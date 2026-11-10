use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
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
                })
                .collect(),
            voice: settings.default_voice.clone(),
            speed: settings.default_speed,
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
}

#[derive(Debug, Clone)]
pub struct ProjectStore {
    data: DataDir,
}

impl ProjectStore {
    pub fn new(data: DataDir) -> Self {
        ProjectStore { data }
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

    pub fn save(&self, p: &mut Project) -> Result<()> {
        p.updated_at = now_secs();
        fsx::write_json_atomic(&self.project_file(&p.id), p)?;
        Ok(())
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
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        match fs::remove_dir_all(self.data.project(id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn summary(&self, p: &Project, settings: &Settings) -> ProjectSummary {
        let progress = match &p.status {
            ProjectStatus::Done { .. } if !p.is_stale(settings) => 1.0,
            ProjectStatus::Draft => 0.0,
            _ => self
                .load_plan(&p.id)
                .ok()
                .flatten()
                .map(|plan| {
                    cache::cached_fraction(
                        &plan,
                        &p.included_ids(),
                        &self.data.project_audio(&p.id),
                    )
                })
                .unwrap_or(0.0),
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
        }
    }

    /// All projects, newest first. Unreadable projects are skipped.
    pub fn list(&self, settings: &Settings) -> Vec<ProjectSummary> {
        let Ok(rd) = fs::read_dir(self.data.projects()) else {
            return Vec::new();
        };
        let mut out: Vec<ProjectSummary> = rd
            .flatten()
            .filter_map(|e| {
                let id = e.file_name().to_string_lossy().into_owned();
                self.load(&id).ok()
            })
            .map(|p| self.summary(&p, settings))
            .collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
        out
    }
}
