//! Tauri commands. Every command runs its core work on the blocking pool and returns
//! `{ message }` errors written for non-developers.

use aloud_core::{
    book::ChapterKind,
    epub, export, fsx, models,
    models::ModelsStatus,
    plan::{self, Plan, Segment, TrickyWord},
    project::{Project, ProjectStatus, ProjectSummary},
    pron::{Dictionary, PronEntry},
    queue::QueueState,
    render::{Direction, RenderProgress, cache},
    settings::{ModelVariant, Performance, Settings, Theme},
    speed, tts,
    update::UpdateState,
    voices::{self, VoiceInfo},
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

use crate::{
    error::{AppError, AppResult, run},
    player::{Cmd, Seg, file_seg},
    state::AppState,
};

type St<'a> = State<'a, Arc<AppState>>;

const WORDS_PER_MINUTE: f64 = 175.0;
const SAMPLE_TEXT: &str = "Once upon a time, in a quiet town by the sea, there lived a girl who loved stories more than anything in the world.";
const BUSY: &str = "Pause first to change this.";
/// How long `quit_app` waits for the job to stop before it refuses to quit.
const QUIT_GRACE: Duration = Duration::from_secs(45);

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(d)?))
}

// ---------- app state & settings ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStateDto {
    settings: Settings,
    models: ModelsStatus,
    queue: QueueState,
    active: Option<RenderProgress>,
    gpu_supported: bool,
    version: String,
    startup_notices: Vec<&'static str>,
    speed_range: speed::SpeedRange,
}

#[tauri::command]
pub async fn get_app_state(state: St<'_>) -> AppResult<AppStateDto> {
    let s = state.inner().clone();
    run(move || {
        Ok(AppStateDto {
            settings: s.settings(),
            models: models::status(&s.data),
            queue: s.render.queue(),
            active: s.render.active(),
            gpu_supported: tts::gpu_supported(),
            speed_range: speed::RANGE,
            version: s.app.package_info().version.to_string(),
            startup_notices: std::mem::take(
                &mut *s
                    .startup_notices
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            ),
        })
    })
    .await
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    model: Option<ModelVariant>,
    output_dir: Option<PathBuf>,
    performance: Option<Performance>,
    gpu: Option<bool>,
    keep_awake: Option<bool>,
    notify_on_finish: Option<bool>,
    check_updates: Option<bool>,
    default_voice: Option<String>,
    default_speed: Option<f32>,
    theme: Option<Theme>,
}

#[tauri::command]
pub async fn update_settings(state: St<'_>, patch: SettingsPatch) -> AppResult<Settings> {
    let s = state.inner().clone();
    run(move || {
        if let Some(v) = &patch.default_voice
            && !voices::is_known(v)
        {
            return Err(AppError::new("That voice isn't available."));
        }
        if let Some(dir) = &patch.output_dir {
            export::check_output_dir(dir)?;
        }
        let (settings, ()) = s.update_settings(|c| {
            if let Some(v) = patch.model {
                c.model = v;
            }
            if let Some(v) = patch.output_dir {
                c.output_dir = v;
            }
            if let Some(v) = patch.performance {
                c.performance = v;
            }
            if let Some(v) = patch.gpu {
                c.gpu = v;
            }
            if let Some(v) = patch.keep_awake {
                c.keep_awake = v;
            }
            if let Some(v) = patch.notify_on_finish {
                c.notify_on_finish = v;
            }
            if let Some(v) = patch.check_updates {
                c.check_updates = v;
            }
            if let Some(v) = patch.default_voice {
                c.default_voice = v;
            }
            if let Some(v) = patch.default_speed {
                c.default_speed = speed::clamp(v);
            }
            if let Some(v) = patch.theme {
                c.theme = v;
            }
            Ok::<_, AppError>(())
        })?;
        Ok(settings)
    })
    .await
}

// ---------- voice packs ----------

/// Clears `AppState::downloading` when the download ends, even by panic.
struct DownloadingGuard<'a>(&'a AtomicBool);

impl Drop for DownloadingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn download_models(state: St<'_>, variant: ModelVariant) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        if s.downloading.swap(true, Ordering::SeqCst) {
            return Err(AppError::new("A download is already in progress."));
        }
        let _guard = DownloadingGuard(&s.downloading);
        s.download_cancel.store(false, Ordering::SeqCst);
        let app = s.app.clone();
        match models::download(&s.data, variant, &s.download_cancel, &|p| {
            let _ = app.emit("model-download", p);
        }) {
            Ok(()) => Ok(()),
            Err(models::DownloadError::Cancelled) => Err(AppError::new("Download cancelled")),
            Err(e) => Err(e.into()),
        }
    })
    .await
}

#[tauri::command]
pub fn cancel_download(state: St<'_>) {
    state.download_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn remove_model(state: St<'_>, variant: ModelVariant) -> AppResult<ModelsStatus> {
    let s = state.inner().clone();
    run(move || {
        if s.downloading.load(Ordering::SeqCst) {
            return Err(AppError::new(
                "Wait for the download to finish, or cancel it, before removing a voice pack.",
            ));
        }
        if s.settings().model == variant {
            return Err(AppError::new(
                "That voice pack is in use. Switch to the other one first.",
            ));
        }
        models::remove(&s.data, variant)?;
        Ok(models::status(&s.data))
    })
    .await
}

#[tauri::command]
pub fn list_voices() -> Vec<VoiceInfo> {
    voices::CATALOG.to_vec()
}

// ---------- projects ----------

#[tauri::command]
pub async fn list_projects(state: St<'_>) -> AppResult<Vec<ProjectSummary>> {
    let s = state.inner().clone();
    run(move || Ok(s.store.list(&s.settings()))).await
}

#[tauri::command]
pub async fn import_epub(state: St<'_>, path: PathBuf) -> AppResult<ProjectSummary> {
    let s = state.inner().clone();
    run(move || {
        let summary = epub::import(&s.data, &path)?;
        s.emit_project(&summary.id);
        Ok(summary)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterDetail {
    id: String,
    title: String,
    kind: ChapterKind,
    included: bool,
    words: u32,
    est_secs: u64,
    rendered_fraction: f32,
    playable: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    project: Project,
    cover_path: Option<PathBuf>,
    word_count: u32,
    estimated_secs: u64,
    chapters: Vec<ChapterDetail>,
    stale: bool,
    /// `None` until a plan matching the current settings exists.
    redo_fraction: Option<f32>,
    output_file_name: String,
}

fn est_secs(words: u32, speed: f32) -> u64 {
    (words as f64 / WORDS_PER_MINUTE * 60.0 / speed.max(0.1) as f64).round() as u64
}

fn detail(s: &AppState, id: &str) -> AppResult<ProjectDetail> {
    let settings = s.settings();
    let project = s.store.load(id)?;
    // Projects imported before the selection carried words and kind still need the book file.
    let legacy = project
        .chapters
        .iter()
        .any(|c| c.words.is_none() || c.kind.is_none());
    let book = if legacy {
        Some(s.store.book(id)?)
    } else {
        None
    };
    let current = plan::compute_inputs_hash(&project, &settings);
    let plan = s
        .store
        .load_plan(id)
        .ok()
        .flatten()
        .filter(|p| p.inputs_hash == current);
    let audio = s.data.project_audio(id);
    let included = project.included_ids();

    let mut word_count = 0;
    let mut estimated = 0;
    let chapters = project
        .chapters
        .iter()
        .map(|sel| {
            let ch = book
                .as_ref()
                .and_then(|b| b.chapters.iter().find(|c| c.id == sel.id));
            let words = sel.words.or(ch.map(|c| c.words)).unwrap_or(0);
            let kind = sel.kind.or(ch.map(|c| c.kind)).unwrap_or(ChapterKind::Body);
            let est = est_secs(words, project.speed);
            if sel.included {
                word_count += words;
                estimated += est;
            }
            let (mut total, mut done, mut speech, mut all) = (0u64, 0u64, 0usize, true);
            if let Some(pc) = plan
                .as_ref()
                .and_then(|p| p.chapters.iter().find(|c| c.chapter_id == sel.id))
            {
                for seg in &pc.segments {
                    if let Segment::Speech { key, units, .. } = seg {
                        speech += 1;
                        total += *units as u64;
                        if cache::exists(&audio, key) {
                            done += *units as u64;
                        } else {
                            all = false;
                        }
                    }
                }
            }
            ChapterDetail {
                id: sel.id.clone(),
                title: sel.title.clone(),
                kind,
                included: sel.included,
                words,
                est_secs: est,
                rendered_fraction: if total == 0 {
                    0.0
                } else {
                    done as f32 / total as f32
                },
                playable: speech > 0 && all,
            }
        })
        .collect();

    let redo_fraction = plan
        .as_ref()
        .map(|p| 1.0 - cache::cached_fraction(p, &included, &audio));
    Ok(ProjectDetail {
        cover_path: project.cover_path(&s.data),
        word_count,
        estimated_secs: estimated,
        chapters,
        stale: project.is_stale(&settings),
        redo_fraction,
        output_file_name: export::output_file_name(&project),
        project,
    })
}

#[tauri::command]
pub async fn get_project(state: St<'_>, id: String) -> AppResult<ProjectDetail> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        detail(&s, &id)
    })
    .await
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MetaPatch {
    title: Option<String>,
    authors: Option<Vec<String>>,
    #[serde(deserialize_with = "double_option")]
    date: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    description: Option<Option<String>>,
}

#[derive(Deserialize)]
struct ChapterPatch {
    id: String,
    title: Option<String>,
    included: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ProjectPatch {
    meta: Option<MetaPatch>,
    voice: Option<String>,
    speed: Option<f32>,
    intro: Option<bool>,
    chapters: Option<Vec<ChapterPatch>>,
}

fn ensure_idle(s: &AppState, id: &str) -> AppResult<()> {
    if s.render.is_active(id) {
        Err(AppError::new(BUSY))
    } else {
        Ok(())
    }
}

/// Rejects ids that aren't real project ids before they reach a filesystem path.
fn check_id(id: &str) -> AppResult<()> {
    if aloud_core::project::is_project_id(id) {
        Ok(())
    } else {
        Err(aloud_core::Error::NotFound(id.to_string()).into())
    }
}

/// Rebuilds an existing plan in the background so "% to redo" is exact once the
/// project-updated event arrives.
fn refresh_plan(s: &AppState, id: &str) {
    if s.store.plan_path(id).exists() {
        s.render.request_replan(id);
    }
}

#[tauri::command]
pub async fn update_project(
    state: St<'_>,
    id: String,
    patch: ProjectPatch,
) -> AppResult<ProjectDetail> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        ensure_idle(&s, &id)?;
        s.store.update(&id, |p| {
            if let Some(m) = patch.meta {
                if let Some(t) = m
                    .title
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                {
                    p.meta.title = t;
                }
                if let Some(a) = m.authors {
                    p.meta.authors = a
                        .into_iter()
                        .map(|a| a.trim().to_string())
                        .filter(|a| !a.is_empty())
                        .collect();
                }
                if let Some(d) = m.date {
                    p.meta.date = d.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
                }
                if let Some(d) = m.description {
                    p.meta.description = d.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
                }
            }
            if let Some(v) = patch.voice {
                if !voices::is_known(&v) {
                    return Err(AppError::new("That voice isn't available."));
                }
                p.voice = v;
            }
            if let Some(v) = patch.speed {
                p.speed = (speed::clamp(v) * 100.0).round() / 100.0;
            }
            if let Some(v) = patch.intro {
                p.intro = v;
            }
            for c in patch.chapters.into_iter().flatten() {
                if let Some(sel) = p.chapters.iter_mut().find(|x| x.id == c.id) {
                    if let Some(t) = c
                        .title
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                    {
                        sel.title = t;
                    }
                    if let Some(i) = c.included {
                        sel.included = i;
                    }
                }
            }
            Ok::<_, AppError>(true)
        })?;
        refresh_plan(&s, &id);
        s.emit_project(&id);
        detail(&s, &id)
    })
    .await
}

#[tauri::command]
pub async fn set_cover(state: St<'_>, id: String, path: PathBuf) -> AppResult<ProjectDetail> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        let mut stale = None;
        s.store.update(&id, |p| {
            stale = epub::set_cover(&s.data, p, &path)?;
            Ok::<_, AppError>(true)
        })?;
        if let Some(old) = stale {
            let _ = std::fs::remove_file(old);
        }
        s.emit_project(&id);
        detail(&s, &id)
    })
    .await
}

#[tauri::command]
pub async fn delete_project(state: St<'_>, id: String) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        s.render.cancel(&id);
        s.player.stop_project(&id);
        s.store.delete(&id)?;
        s.emit_project(&id);
        Ok(())
    })
    .await
}

// ---------- previews & pronunciations ----------

fn require_models(s: &AppState) -> AppResult<()> {
    if models::status(&s.data).ready(s.settings().model) {
        Ok(())
    } else {
        Err(AppError::new("Download a voice pack first."))
    }
}

fn play_preview(
    s: &AppState,
    project_id: Option<&str>,
    voice: &str,
    speed: f32,
    text: &str,
) -> AppResult<()> {
    require_models(s)?;
    let pcm = s.render.preview(project_id, voice, speed, text)?;
    s.player.send(Cmd::Play {
        project_id: None,
        chapter_id: None,
        title: "Preview".into(),
        segs: vec![Seg::Mem(pcm.into())],
        start_ms: 0,
    });
    Ok(())
}

#[tauri::command]
pub async fn preview_voice(
    state: St<'_>,
    project_id: Option<String>,
    voice: String,
    speed: f32,
    text: Option<String>,
) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        if let Some(id) = &project_id {
            check_id(id)?;
        }
        if !voices::is_known(&voice) {
            return Err(AppError::new("That voice isn't available."));
        }
        let text = text
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| SAMPLE_TEXT.into());
        play_preview(&s, project_id.as_deref(), &voice, speed, &text)
    })
    .await
}

#[tauri::command]
pub async fn test_pronunciation(state: St<'_>, project_id: String, text: String) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        check_id(&project_id)?;
        let p = s.store.load(&project_id)?;
        play_preview(&s, Some(&project_id), &p.voice, p.speed, &text)
    })
    .await
}

#[tauri::command]
pub async fn get_tricky_words(state: St<'_>, id: String) -> AppResult<Vec<TrickyWord>> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        let plan = s
            .render
            .ensure_plan(&id, &|_, _| {}, &AtomicBool::new(false))?;
        let p = s.store.load(&id)?;
        let book = s.store.book(&id)?;
        Ok(plan::tricky_words(&book, &plan, &p.dismissed_words))
    })
    .await
}

#[tauri::command]
pub async fn dismiss_word(state: St<'_>, id: String, word: String) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        let w = word.trim().to_lowercase();
        s.store.update(&id, |p| {
            let add = !w.is_empty() && !p.dismissed_words.contains(&w);
            if add {
                p.dismissed_words.push(w.clone());
            }
            Ok::<_, AppError>(add)
        })?;
        Ok(())
    })
    .await
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    Book,
    Global,
}

fn upsert_entry(list: &mut Vec<PronEntry>, entry: PronEntry) {
    match list.iter().position(|e| e.id == entry.id) {
        Some(i) => list[i] = entry.clone(),
        None => list.push(entry.clone()),
    }
    list.retain(|e| e.id == entry.id || !e.word.eq_ignore_ascii_case(&entry.word));
}

/// Applies `f` to the pronunciation list of `scope` and returns the resulting list.
fn edit_prons(
    s: &AppState,
    scope: Scope,
    project_id: Option<&str>,
    f: impl FnOnce(&mut Vec<PronEntry>),
) -> AppResult<Vec<PronEntry>> {
    match scope {
        Scope::Global => {
            let book = match project_id {
                Some(id) => s.store.load(id)?.pronunciations,
                None => Vec::new(),
            };
            let (settings, ()) = s.update_settings(|c| {
                f(&mut c.pronunciations);
                Dictionary::new(&c.pronunciations, &book)?;
                Ok::<_, AppError>(())
            })?;
            if let Some(id) = project_id {
                refresh_plan(s, id);
                s.emit_project(id);
            }
            Ok(settings.pronunciations)
        }
        Scope::Book => {
            let id = project_id.ok_or_else(|| AppError::new("Choose a book first."))?;
            ensure_idle(s, id)?;
            let settings = s.settings();
            let p = s.store.update(id, |p| {
                f(&mut p.pronunciations);
                Dictionary::new(&settings.pronunciations, &p.pronunciations)?;
                Ok::<_, AppError>(true)
            })?;
            refresh_plan(s, id);
            s.emit_project(id);
            Ok(p.pronunciations)
        }
    }
}

#[tauri::command]
pub async fn upsert_pronunciation(
    state: St<'_>,
    scope: Scope,
    project_id: Option<String>,
    mut entry: PronEntry,
) -> AppResult<Vec<PronEntry>> {
    let s = state.inner().clone();
    run(move || {
        if let Some(id) = &project_id {
            check_id(id)?;
        }
        entry.word = entry.word.trim().to_string();
        entry.validate()?;
        edit_prons(&s, scope, project_id.as_deref(), |l| upsert_entry(l, entry))
    })
    .await
}

#[tauri::command]
pub async fn delete_pronunciation(
    state: St<'_>,
    scope: Scope,
    project_id: Option<String>,
    entry_id: String,
) -> AppResult<Vec<PronEntry>> {
    let s = state.inner().clone();
    run(move || {
        if let Some(id) = &project_id {
            check_id(id)?;
        }
        edit_prons(&s, scope, project_id.as_deref(), |l| {
            l.retain(|e| e.id != entry_id)
        })
    })
    .await
}

// ---------- render queue ----------

#[tauri::command]
pub async fn enqueue(state: St<'_>, id: String) -> AppResult<QueueState> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        require_models(&s)?;
        export::check_output_dir(&s.settings().output_dir)?;
        Ok(s.render.enqueue(&id)?)
    })
    .await
}

#[tauri::command]
pub async fn cancel_render(state: St<'_>, id: String) -> AppResult<QueueState> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        Ok(s.render.cancel(&id))
    })
    .await
}

#[tauri::command]
pub async fn pause_queue(state: St<'_>) -> AppResult<QueueState> {
    let s = state.inner().clone();
    run(move || Ok(s.render.pause())).await
}

#[tauri::command]
pub async fn resume_queue(state: St<'_>) -> AppResult<QueueState> {
    let s = state.inner().clone();
    run(move || Ok(s.render.resume())).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Dir {
    Up,
    Down,
}

#[tauri::command]
pub async fn move_in_queue(state: St<'_>, id: String, direction: Dir) -> AppResult<QueueState> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        let d = match direction {
            Dir::Up => Direction::Up,
            Dir::Down => Direction::Down,
        };
        Ok(s.render.move_item(&id, d))
    })
    .await
}

// ---------- player ----------

fn chapter_segs(
    plan: &Plan,
    project: &Project,
    chapter_id: &str,
) -> Option<(String, Vec<Segment>)> {
    let included = project.included_ids();
    let seq = plan.sequence(&included);
    if let Some(c) = seq.iter().find(|c| c.id == chapter_id) {
        return Some((
            c.title.to_string(),
            c.segments.iter().map(|s| (*s).clone()).collect(),
        ));
    }
    plan.chapters
        .iter()
        .find(|c| c.chapter_id == chapter_id)
        .map(|c| (c.title.clone(), c.segments.clone()))
}

#[tauri::command]
pub async fn player_play_chapter(
    state: St<'_>,
    project_id: String,
    chapter_id: String,
    start_ms: u64,
) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        check_id(&project_id)?;
        let settings = s.settings();
        let project = s.store.load(&project_id)?;
        let hash = plan::compute_inputs_hash(&project, &settings);
        let plan = s
            .store
            .load_plan(&project_id)
            .ok()
            .flatten()
            .filter(|p| p.inputs_hash == hash)
            .ok_or_else(|| AppError::new("That chapter isn't ready yet."))?;
        let (title, segments) = chapter_segs(&plan, &project, &chapter_id)
            .ok_or_else(|| AppError::new("That chapter isn't ready yet."))?;
        let audio = s.data.project_audio(&project_id);
        let mut segs = Vec::with_capacity(segments.len());
        for seg in &segments {
            segs.push(match seg {
                Segment::Silence { ms } => {
                    Seg::Silence(*ms as u64 * aloud_core::SAMPLE_RATE as u64 / 1000)
                }
                Segment::Speech { key, .. } => file_seg(&audio, key)
                    .ok_or_else(|| AppError::new("That chapter isn't ready yet."))?,
            });
        }
        s.player.send(Cmd::Play {
            project_id: Some(project_id),
            chapter_id: Some(chapter_id),
            title,
            segs,
            start_ms,
        });
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn player_toggle(state: St<'_>) {
    state.player.send(Cmd::Toggle);
}

#[tauri::command]
pub fn player_stop(state: St<'_>) {
    state.player.send(Cmd::Stop);
}

#[tauri::command]
pub fn player_seek(state: St<'_>, ms: u64) {
    state.player.send(Cmd::Seek(ms));
}

// ---------- updates ----------

const UPDATE_BUSY: &str = "Pause the queue first to install the update.";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UpdateProgress {
    downloaded: u64,
    total: Option<u64>,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Looks for a newer release. Automatic checks (`force == false`) honor the opt-out, the 24 h
/// throttle and the skipped version, and stay silent on failure; a forced check does none of that.
#[tauri::command]
pub async fn check_for_update(state: St<'_>, force: bool) -> AppResult<Option<UpdateInfo>> {
    let s = state.inner().clone();
    let (enabled, mut us) = {
        let s = s.clone();
        run(move || Ok((s.settings().check_updates, UpdateState::load(&s.data)?))).await?
    };
    let now = unix_now();
    if !force && (!enabled || !us.is_due(now)) {
        return Ok(None);
    }
    let checked = match s.app.updater() {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    let found = match checked {
        Ok(found) => found,
        Err(_) if force => {
            return Err(AppError::new(
                "Couldn't check for updates. Check your internet connection and try again.",
            ));
        }
        // Leave `checked_at` alone so the next launch retries.
        Err(_) => return Ok(None),
    };
    let offered = found.filter(|u| force || !us.is_skipped(&u.version));
    let Some(u) = offered else {
        *s.pending_update.lock().unwrap_or_else(|e| e.into_inner()) = None;
        us.checked_at = now;
        let data = s.data.clone();
        run(move || Ok(us.save(&data)?)).await?;
        return Ok(None);
    };
    let info = UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
    };
    *s.pending_update.lock().unwrap_or_else(|e| e.into_inner()) = Some(u);
    Ok(Some(info))
}

#[tauri::command]
pub async fn skip_update(state: St<'_>, version: String) -> AppResult<()> {
    let s = state.inner().clone();
    *s.pending_update.lock().unwrap_or_else(|e| e.into_inner()) = None;
    run(move || {
        let mut us = UpdateState::load(&s.data)?;
        us.skipped_version = Some(version);
        us.checked_at = unix_now();
        Ok(us.save(&s.data)?)
    })
    .await
}

#[tauri::command]
pub async fn install_update(state: St<'_>) -> AppResult<()> {
    let s = state.inner().clone();
    if s.render.is_busy() {
        return Err(AppError::new(UPDATE_BUSY));
    }
    let pending = s
        .pending_update
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let Some(update) = pending else {
        return Err(AppError::new("Check for updates again, then install."));
    };
    let mut downloaded = 0u64;
    let mut last_emit: Option<Instant> = None;
    let app = s.app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                if last_emit.is_none_or(|t| t.elapsed() >= Duration::from_millis(200)) {
                    last_emit = Some(Instant::now());
                    let _ = app.emit("update-progress", UpdateProgress { downloaded, total });
                }
            },
            || {},
        )
        .await
        .map_err(|e| AppError::new(format!("The update couldn't be installed: {e}")))?;
    s.app.restart()
}

// ---------- storage ----------

#[tauri::command]
pub async fn project_cache_size(state: St<'_>, id: String) -> AppResult<u64> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        Ok(fsx::dir_size(&s.data.project_audio(&id)))
    })
    .await
}

#[tauri::command]
pub async fn clear_project_cache(state: St<'_>, id: String) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        ensure_idle(&s, &id)?;
        s.player.stop_project(&id);
        match std::fs::remove_dir_all(s.data.project_audio(&id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        s.store.forget_progress(&id);
        // Done stays Done: the exported file is untouched.
        s.store.update(&id, |p| {
            Ok::<_, AppError>(matches!(
                p.status,
                ProjectStatus::Queued | ProjectStatus::Paused
            ))
        })?;
        s.emit_project(&id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn storage_size(state: St<'_>) -> AppResult<u64> {
    let s = state.inner().clone();
    run(move || {
        let projects = std::fs::read_dir(s.data.projects())
            .map(|rd| {
                rd.flatten()
                    .map(|e| fsx::dir_size(&e.path().join("audio")))
                    .sum::<u64>()
            })
            .unwrap_or(0);
        Ok(projects + fsx::dir_size(&s.data.previews()))
    })
    .await
}

#[tauri::command]
pub async fn open_data_dir(state: St<'_>) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        std::fs::create_dir_all(s.data.root())?;
        s.app
            .opener()
            .open_path(s.data.root().to_string_lossy(), None::<&str>)
            .map_err(|e| AppError::new(e.to_string()))
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AboutDoc {
    Source,
    License,
    Notices,
}

/// Opens the source repository in the browser, or a licence document shipped with the app.
#[tauri::command]
pub async fn open_about(state: St<'_>, doc: AboutDoc) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        let opener = s.app.opener();
        let target = match doc {
            AboutDoc::Source => {
                return opener
                    .open_url("https://github.com/suft/aloud", None::<&str>)
                    .map_err(|e| AppError::new(e.to_string()));
            }
            AboutDoc::License => "LICENSE.txt",
            AboutDoc::Notices => "THIRD-PARTY-NOTICES.txt",
        };
        let path = s
            .app
            .path()
            .resource_dir()
            .map_err(|e| AppError::new(e.to_string()))?
            .join(target);
        if !path.is_file() {
            return Err(AppError::new("That file isn't included in this build."));
        }
        opener
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|e| AppError::new(e.to_string()))
    })
    .await
}

#[tauri::command]
pub async fn open_output(state: St<'_>, id: String) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        check_id(&id)?;
        let ProjectStatus::Done { output, .. } = s.store.load(&id)?.status else {
            return Err(AppError::new("This audiobook isn't finished yet."));
        };
        if !output.is_file() {
            return Err(AppError::new(
                "The audiobook file is missing. It may have been moved or deleted.",
            ));
        }
        s.app
            .opener()
            .open_path(output.to_string_lossy(), None::<&str>)
            .map_err(|e| AppError::new(e.to_string()))
    })
    .await
}

#[tauri::command]
pub async fn quit_app(state: St<'_>) -> AppResult<()> {
    let s = state.inner().clone();
    run(move || {
        // Pausing stops the job after its current utterance; an export stops on the same flag
        // and deletes its partial file, so a completed pause never leaves a half-written output.
        s.render.pause();
        if !s.render.wait_idle(QUIT_GRACE) {
            return Err(AppError::new(
                "Aloud is still saving your audiobook. Try quitting again in a minute.",
            ));
        }
        s.app.exit(0);
        Ok(())
    })
    .await
}
