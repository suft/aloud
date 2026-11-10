use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{
        Arc, LazyLock, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    book::{Book, BookMeta, ChapterKind},
    error::{Error, Result},
    fsx,
    paths::DataDir,
    plan::{self, Plan},
    pron::PronEntry,
    render::cache,
    settings::Settings,
};

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterSel {
    pub id: String,
    pub title: String,
    pub included: bool,
    /// Copied from the book at import so the book page needn't read `book.json`; `None` for
    /// projects imported before that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ChapterKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ProjectStatus {
    Draft,
    Queued,
    Rendering,
    Paused,
    Exporting,
    #[serde(rename_all = "camelCase")]
    Done {
        output: PathBuf,
        finished_at: i64,
        /// Signature of everything that shaped the export; see [`Project::export_signature`].
        inputs_hash: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub source_file_name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub meta: BookMeta,
    pub chapters: Vec<ChapterSel>,
    pub voice: String,
    pub speed: f32,
    pub intro: bool,
    pub pronunciations: Vec<PronEntry>,
    pub dismissed_words: Vec<String>,
    pub status: ProjectStatus,
}

impl Project {
    pub fn new(id: &str, source_file_name: String, book: &Book, settings: &Settings) -> Self {
        let now = now_secs();
        Project {
            id: id.to_string(),
            source_file_name,
            created_at: now,
            updated_at: now,
            meta: book.meta.clone(),
            chapters: book
                .chapters
                .iter()
                .map(|c| ChapterSel {
                    id: c.id.clone(),
                    title: c.title.clone(),
                    included: c.kind == ChapterKind::Body,
                    words: Some(c.words),
                    kind: Some(c.kind),
                })
                .collect(),
            voice: settings.default_voice.clone(),
            speed: crate::speed::clamp(settings.default_speed),
            intro: true,
            pronunciations: Vec::new(),
            dismissed_words: Vec::new(),
            status: ProjectStatus::Draft,
        }
    }

    pub fn included_ids(&self) -> HashSet<String> {
        self.chapters
            .iter()
            .filter(|c| c.included)
            .map(|c| c.id.clone())
            .collect()
    }

    /// Hash of the plan inputs plus the included chapter set; stored in `Done` to detect staleness.
    pub fn export_signature(&self, plan_hash: &str) -> String {
        let mut h = blake3::Hasher::new();
        h.update(plan_hash.as_bytes());
        for c in self.chapters.iter().filter(|c| c.included) {
            h.update(b"|");
            h.update(c.id.as_bytes());
        }
        h.finalize().to_hex().to_string()
    }

    /// Done, but the settings changed since the export.
    pub fn is_stale(&self, settings: &Settings) -> bool {
        match &self.status {
            ProjectStatus::Done { inputs_hash, .. } => {
                let now = self.export_signature(&plan::compute_inputs_hash(self, settings));
                &now != inputs_hash
            }
            _ => false,
        }
    }

    pub fn cover_path(&self, data: &DataDir) -> Option<PathBuf> {
        self.meta
            .cover_file
            .as_ref()
            .map(|f| data.project(&self.id).join(f))
            .filter(|p| p.exists())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub cover_path: Option<PathBuf>,
    pub accent: Option<String>,
    pub status: ProjectStatus,
    pub progress: f32,
    pub updated_at: i64,
    /// The project's data can't be read; the only useful action is to remove it.
    #[serde(default)]
    pub damaged: bool,
}

impl ProjectSummary {
    fn damaged(id: String) -> Self {
        ProjectSummary {
            id,
            title: String::new(),
            authors: vec![],
            cover_path: None,
            accent: None,
            status: ProjectStatus::Draft,
            progress: 0.0,
            updated_at: 0,
            damaged: true,
        }
    }
}

/// Project ids are the first 16 lowercase hex digits of the EPUB's blake3 hash (`epub::import`).
pub fn is_project_id(id: &str) -> bool {
    id.len() == 16 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// One lock per project directory, shared by every `ProjectStore` in the process (the app state,
/// the render service and the importer each build their own store).
static PROJECT_LOCKS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
    LazyLock::new(Default::default);

/// Library progress per project directory; see [`ProjectStore::cached_progress`].
static PROGRESS: LazyLock<Mutex<HashMap<PathBuf, f32>>> = LazyLock::new(Default::default);
/// Bumped by every `forget_progress`, so a value computed across a forget is not stored.
static PROGRESS_EPOCH: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct ProjectStore {
    data: DataDir,
}

impl ProjectStore {
    pub fn new(data: DataDir) -> Self {
        ProjectStore { data }
    }

    fn project_lock(&self, id: &str) -> Arc<Mutex<()>> {
        PROJECT_LOCKS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(self.data.project(id))
            .or_default()
            .clone()
    }

    pub fn data(&self) -> &DataDir {
        &self.data
    }

    fn project_file(&self, id: &str) -> PathBuf {
        self.data.project(id).join("project.json")
    }

    pub fn exists(&self, id: &str) -> bool {
        self.project_file(id).exists()
    }

    pub fn load(&self, id: &str) -> Result<Project> {
        fsx::read_json(&self.project_file(id))?.ok_or_else(|| Error::NotFound(id.to_string()))
    }

    fn write(&self, p: &mut Project) -> Result<()> {
        p.updated_at = now_secs();
        fsx::write_json_atomic(&self.project_file(&p.id), p)?;
        self.forget_progress(&p.id);
        Ok(())
    }

    /// Writes a new project. Existing projects go through `update`.
    pub fn save(&self, p: &mut Project) -> Result<()> {
        let lock = self.project_lock(&p.id);
        let _g = lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.write(p)
    }

    /// Loads project `id`, lets `f` change it and saves it if `f` returns true, holding the
    /// project's lock so concurrent writers can't interleave or overwrite each other.
    /// Returns the project as stored. Never call another `ProjectStore` method inside `f`.
    pub fn update<E: From<Error>>(
        &self,
        id: &str,
        f: impl FnOnce(&mut Project) -> std::result::Result<bool, E>,
    ) -> std::result::Result<Project, E> {
        let lock = self.project_lock(id);
        let _g = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut p = self.load(id)?;
        if f(&mut p)? {
            self.write(&mut p)?;
        }
        Ok(p)
    }

    pub fn book(&self, id: &str) -> Result<Book> {
        let path = self.data.project(id).join("book.json");
        fsx::read_json(&path)?.ok_or_else(|| Error::NotFound(id.to_string()))
    }

    pub fn save_book(&self, id: &str, book: &Book) -> Result<()> {
        fsx::write_json_atomic(&self.data.project(id).join("book.json"), book)?;
        Ok(())
    }

    pub fn plan_path(&self, id: &str) -> PathBuf {
        self.data.project(id).join("plan.json")
    }

    pub fn load_plan(&self, id: &str) -> Result<Option<Plan>> {
        fsx::read_json(&self.plan_path(id))
    }

    pub fn save_plan(&self, id: &str, plan: &Plan) -> Result<()> {
        fsx::write_json_atomic(&self.plan_path(id), plan)?;
        self.forget_progress(id);
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let lock = self.project_lock(id);
        let _g = lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.forget_progress(id);
        match fs::remove_dir_all(self.data.project(id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Drops the remembered library progress of `id`; call after anything that changes which
    /// audio exists or which segments are needed.
    pub fn forget_progress(&self, id: &str) {
        let mut map = PROGRESS.lock().unwrap_or_else(PoisonError::into_inner);
        PROGRESS_EPOCH.fetch_add(1, Ordering::SeqCst);
        map.remove(&self.data.project(id));
    }

    /// Share of the included speech that is cached. Computing it stats one file per segment,
    /// so the value is kept until the project is written or its plan or audio changes.
    fn cached_progress(&self, p: &Project) -> f32 {
        let key = self.data.project(&p.id);
        let epoch = PROGRESS_EPOCH.load(Ordering::SeqCst);
        if let Some(v) = PROGRESS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return *v;
        }
        let v = self
            .load_plan(&p.id)
            .ok()
            .flatten()
            .map(|plan| {
                cache::cached_fraction(&plan, &p.included_ids(), &self.data.project_audio(&p.id))
            })
            .unwrap_or(0.0);
        let mut map = PROGRESS.lock().unwrap_or_else(PoisonError::into_inner);
        // A forget since `epoch` means `v` may describe state that no longer exists.
        if PROGRESS_EPOCH.load(Ordering::SeqCst) == epoch {
            map.insert(key, v);
        }
        v
    }

    pub fn summary(&self, p: &Project, settings: &Settings) -> ProjectSummary {
        let progress = match &p.status {
            ProjectStatus::Done { .. } if !p.is_stale(settings) => 1.0,
            ProjectStatus::Draft => 0.0,
            _ => self.cached_progress(p),
        };
        ProjectSummary {
            id: p.id.clone(),
            title: p.meta.title.clone(),
            authors: p.meta.authors.clone(),
            cover_path: p.cover_path(&self.data),
            accent: p.meta.accent.clone(),
            status: p.status.clone(),
            progress,
            updated_at: p.updated_at,
            damaged: false,
        }
    }

    /// All projects, newest first. A project whose data can't be read is listed as damaged
    /// (last) so it can be removed; a directory without `project.json` is an import in progress.
    pub fn list(&self, settings: &Settings) -> Vec<ProjectSummary> {
        let Ok(rd) = fs::read_dir(self.data.projects()) else {
            return Vec::new();
        };
        let mut out: Vec<ProjectSummary> = rd
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| is_project_id(&e.file_name().to_string_lossy()))
            .filter_map(|e| {
                let id = e.file_name().to_string_lossy().into_owned();
                match self.load(&id) {
                    Ok(p) => Some(self.summary(&p, settings)),
                    Err(Error::NotFound(_)) => None,
                    Err(_) => Some(ProjectSummary::damaged(id)),
                }
            })
            .collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_projects_are_listed_as_damaged() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path().to_path_buf());
        let store = ProjectStore::new(data.clone());
        let settings = Settings::default();
        let book = Book {
            meta: BookMeta::default(),
            chapters: vec![],
        };
        let (good_id, bad_id, partial_id) =
            ("0000000000000001", "0000000000000002", "0000000000000003");
        let mut good = Project::new(good_id, "b.epub".into(), &book, &settings);
        store.save(&mut good).unwrap();

        fs::create_dir_all(data.project(bad_id)).unwrap();
        fs::write(data.project(bad_id).join("project.json"), "{nope").unwrap();
        fs::create_dir_all(data.project(partial_id)).unwrap();
        fs::write(data.projects().join("stray.txt"), "x").unwrap();
        // Not a project id, so never listed (or opened) even with a broken project.json.
        fs::create_dir_all(data.project("not-a-project")).unwrap();
        fs::write(data.project("not-a-project").join("project.json"), "{nope").unwrap();

        let list = store.list(&settings);
        let ids: Vec<(&str, bool)> = list.iter().map(|s| (s.id.as_str(), s.damaged)).collect();
        assert_eq!(ids, [(good_id, false), (bad_id, true)]);
    }

    #[test]
    fn project_ids_must_be_16_lowercase_hex() {
        assert!(is_project_id("0123456789abcdef"));
        for bad in [
            "",
            "0123456789ABCDEF",
            "../0123456789abc",
            "/etc/passwd",
            "0123456789abcde",
            "0123456789abcdef0",
            "..\\..\\x",
            "0123456789abcdeg",
        ] {
            assert!(!is_project_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn concurrent_updates_never_lose_writes() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path().to_path_buf());
        let id = "0000000000000001";
        let book = Book {
            meta: BookMeta::default(),
            chapters: vec![],
        };
        let mut p = Project::new(id, "b.epub".into(), &book, &Settings::default());
        ProjectStore::new(data.clone()).save(&mut p).unwrap();

        let mut threads = Vec::new();
        for t in 0..8 {
            let store = ProjectStore::new(data.clone());
            threads.push(std::thread::spawn(move || {
                for i in 0..50 {
                    store
                        .update(id, |p| {
                            p.dismissed_words.push(format!("{t}-{i}"));
                            Ok::<_, Error>(true)
                        })
                        .unwrap();
                }
            }));
        }
        let store = ProjectStore::new(data.clone());
        threads.push(std::thread::spawn(move || {
            for i in 0..50 {
                store
                    .update(id, |p| {
                        p.status = if i % 2 == 0 {
                            ProjectStatus::Queued
                        } else {
                            ProjectStatus::Paused
                        };
                        Ok::<_, Error>(true)
                    })
                    .unwrap();
            }
        }));
        for t in threads {
            t.join().unwrap();
        }

        let store = ProjectStore::new(data.clone());
        assert_eq!(store.load(id).unwrap().dismissed_words.len(), 400);
        let leftovers = fs::read_dir(data.project(id))
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .count();
        assert_eq!(leftovers, 0);
    }
}
