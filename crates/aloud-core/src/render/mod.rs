//! Render service: queue, scheduler, workers, progress, previews.

pub mod cache;
mod job;
mod preview;

use serde::Serialize;
use std::{
    panic::{self, AssertUnwindSafe},
    sync::{
        Arc, Condvar, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub use crate::export::FinishedInfo;
use crate::{
    error::{Error, Result},
    g2p::{Phonemize, Phonemizer},
    models,
    paths::DataDir,
    plan::{self, Plan},
    project::{ProjectStatus, ProjectStore},
    queue::QueueState,
    settings::{ModelVariant, Settings},
    tts::{Engine, EngineOptions, Synth},
    voices::Voices,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Planning,
    Rendering,
    Exporting,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterProgress {
    pub id: String,
    pub done_units: u64,
    pub total_units: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderProgress {
    pub project_id: String,
    pub phase: Phase,
    pub done_units: u64,
    pub total_units: u64,
    pub fraction: f32,
    pub chapters: Vec<ChapterProgress>,
    pub eta_secs: Option<u64>,
    pub realtime_factor: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

pub trait Events: Send + Sync {
    fn progress(&self, p: &RenderProgress);
    fn project_changed(&self, id: &str);
    fn queue_changed(&self, q: &QueueState);
    fn finished(&self, id: &str, title: &str, result: &std::result::Result<FinishedInfo, String>);
    /// Out-of-band notices; currently `"gpu-fallback"`.
    fn notice(&self, code: &str);
}

pub(crate) enum JobEnd {
    Done(FinishedInfo),
    /// Paused or cancelled; finished parts are kept.
    Interrupted,
    Failed(String),
    DiskFull,
}

struct ActiveJob {
    id: String,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct State {
    queue: QueueState,
    active: Option<ActiveJob>,
    progress: Option<RenderProgress>,
}

/// Opens one synthesis engine for a worker; the bool reports a GPU → CPU fallback.
pub(crate) type OpenEngine = Box<
    dyn Fn(&RenderService, ModelVariant, EngineOptions) -> Result<(Box<dyn Synth>, bool)>
        + Send
        + Sync,
>;

pub struct RenderService {
    pub(crate) data: DataDir,
    pub(crate) store: ProjectStore,
    pub(crate) events: Arc<dyn Events>,
    state: Mutex<State>,
    cv: Condvar,
    plan_lock: Mutex<()>,
    pub(crate) phonemizer: Box<dyn Phonemize>,
    open_engine: OpenEngine,
    voices: Mutex<Option<Arc<Voices>>>,
    preview: Mutex<preview::PreviewState>,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Human-readable text of a caught panic payload.
pub(crate) fn panic_text(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unexpected internal error")
}

fn open_kokoro(
    svc: &RenderService,
    model: ModelVariant,
    opts: EngineOptions,
) -> Result<(Box<dyn Synth>, bool)> {
    let (engine, fell_back) =
        Engine::open(&models::model_path(&svc.data, model), svc.voices()?, opts)?;
    Ok((Box::new(engine), fell_back))
}

impl RenderService {
    /// Creates the service and starts its scheduler thread. Rendering never starts by itself:
    /// a non-empty persisted queue comes back paused.
    pub fn new(data: DataDir, events: Arc<dyn Events>) -> Arc<Self> {
        Self::with_backend(
            data,
            events,
            Box::new(Phonemizer::new()),
            Box::new(open_kokoro),
        )
    }

    /// `new` with the phonemizer and the engine factory supplied, so tests can run without models.
    pub(crate) fn with_backend(
        data: DataDir,
        events: Arc<dyn Events>,
        phonemizer: Box<dyn Phonemize>,
        open_engine: OpenEngine,
    ) -> Arc<Self> {
        let store = ProjectStore::new(data.clone());
        let mut queue = QueueState::load(&data).unwrap_or_default();
        queue.items.retain(|id| store.exists(id));
        queue.paused = !queue.items.is_empty();
        let _ = queue.save(&data);

        // Reconcile project statuses with the queue.
        if let Ok(rd) = std::fs::read_dir(data.projects()) {
            for entry in rd.flatten() {
                let id = entry.file_name().to_string_lossy().into_owned();
                let _ = store.update(&id, |p| {
                    let in_queue = queue.items.contains(&id);
                    let new = match p.status {
                        ProjectStatus::Rendering
                        | ProjectStatus::Exporting
                        | ProjectStatus::Queued => Some(if in_queue {
                            ProjectStatus::Queued
                        } else {
                            ProjectStatus::Paused
                        }),
                        _ => None,
                    };
                    if let Some(s) = &new {
                        p.status = s.clone();
                    }
                    Ok::<_, Error>(new.is_some())
                });
            }
        }

        let svc = Arc::new(RenderService {
            data,
            store,
            events,
            state: Mutex::new(State {
                queue,
                ..State::default()
            }),
            cv: Condvar::new(),
            plan_lock: Mutex::new(()),
            phonemizer,
            open_engine,
            voices: Mutex::new(None),
            preview: Mutex::new(preview::PreviewState::default()),
        });
        let worker = svc.clone();
        thread::Builder::new()
            .name("aloud-scheduler".into())
            .spawn(move || worker.scheduler())
            .expect("spawn scheduler thread");
        svc
    }

    pub fn data(&self) -> &DataDir {
        &self.data
    }

    pub fn store(&self) -> &ProjectStore {
        &self.store
    }

    pub fn queue(&self) -> QueueState {
        lock(&self.state).queue.clone()
    }

    pub fn active(&self) -> Option<RenderProgress> {
        lock(&self.state).progress.clone()
    }

    /// True while a job is running (not paused).
    pub fn is_busy(&self) -> bool {
        let st = lock(&self.state);
        st.active.is_some() && !st.queue.paused
    }

    pub fn is_active(&self, id: &str) -> bool {
        lock(&self.state)
            .active
            .as_ref()
            .is_some_and(|a| a.id == id)
    }

    fn persist_and_emit(&self, st: &State) {
        let _ = st.queue.save(&self.data);
        self.events.queue_changed(&st.queue);
    }

    pub(crate) fn set_status(&self, id: &str, status: ProjectStatus) {
        let saved = self.store.update(id, |p| {
            p.status = status;
            Ok::<_, Error>(true)
        });
        if saved.is_ok() {
            self.events.project_changed(id);
        }
    }

    pub fn enqueue(&self, id: &str) -> Result<QueueState> {
        self.store.update(id, |p| {
            let idle = !matches!(
                p.status,
                ProjectStatus::Rendering | ProjectStatus::Exporting
            );
            if idle {
                p.status = ProjectStatus::Queued;
            }
            Ok::<_, Error>(idle)
        })?;
        self.events.project_changed(id);
        let mut st = lock(&self.state);
        if !st.queue.items.iter().any(|i| i == id) {
            st.queue.items.push(id.to_string());
        }
        self.persist_and_emit(&st);
        let q = st.queue.clone();
        drop(st);
        self.cv.notify_all();
        Ok(q)
    }

    /// Stops the running job (if any) and keeps the queue paused. Blocks until the workers
    /// have stopped, which takes at most one utterance.
    pub fn pause(&self) -> QueueState {
        let mut st = lock(&self.state);
        st.queue.paused = true;
        if let Some(a) = &st.active {
            a.cancel.store(true, Ordering::SeqCst);
        }
        self.persist_and_emit(&st);
        let q = st.queue.clone();
        drop(st);
        self.wait_idle(Duration::from_secs(15));
        q
    }

    pub fn resume(&self) -> QueueState {
        let mut st = lock(&self.state);
        st.queue.paused = false;
        self.persist_and_emit(&st);
        let q = st.queue.clone();
        drop(st);
        self.cv.notify_all();
        q
    }

    /// Removes `id` from the queue. A running job is stopped; finished parts are kept.
    pub fn cancel(&self, id: &str) -> QueueState {
        let mut st = lock(&self.state);
        st.queue.items.retain(|i| i != id);
        let running = st.active.as_ref().is_some_and(|a| a.id == id);
        if running && let Some(a) = &st.active {
            a.cancel.store(true, Ordering::SeqCst);
        }
        self.persist_and_emit(&st);
        let q = st.queue.clone();
        drop(st);
        if running {
            self.wait_idle(Duration::from_secs(15));
        } else {
            let mut changed = false;
            let _ = self.store.update(id, |p| {
                changed = matches!(p.status, ProjectStatus::Queued);
                if changed {
                    p.status = ProjectStatus::Paused;
                }
                Ok::<_, Error>(changed)
            });
            if changed {
                self.events.project_changed(id);
            }
        }
        q
    }

    pub fn move_item(&self, id: &str, dir: Direction) -> QueueState {
        let mut st = lock(&self.state);
        let locked = usize::from(st.active.is_some());
        if let Some(i) = st.queue.items.iter().position(|x| x == id) {
            let j = match dir {
                Direction::Up => i.checked_sub(1),
                Direction::Down => (i + 1 < st.queue.items.len()).then_some(i + 1),
            };
            if let Some(j) = j.filter(|&j| i >= locked && j >= locked) {
                st.queue.items.swap(i, j);
                self.persist_and_emit(&st);
            }
        }
        st.queue.clone()
    }

    /// Blocks until no job is running or `timeout` passes.
    pub fn wait_idle(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let mut st = lock(&self.state);
        while st.active.is_some() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            st = self
                .cv
                .wait_timeout(st, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    pub(crate) fn voices(&self) -> Result<Arc<Voices>> {
        let mut slot = lock(&self.voices);
        if let Some(v) = slot.as_ref() {
            return Ok(v.clone());
        }
        let v = Arc::new(Voices::load(&models::voices_path(&self.data))?);
        *slot = Some(v.clone());
        Ok(v)
    }

    pub(crate) fn set_progress(&self, p: RenderProgress) {
        self.events.progress(&p);
        lock(&self.state).progress = Some(p);
    }

    /// Builds (or loads) the plan for a project. Serialized, so only one build runs at a time.
    pub fn ensure_plan(&self, id: &str, progress: &dyn Fn(usize, usize)) -> Result<Plan> {
        let _guard = lock(&self.plan_lock);
        let project = self.store.load(id)?;
        let settings = Settings::load(&self.data)?;
        let hash = plan::compute_inputs_hash(&project, &settings);
        if let Some(p) = self.store.load_plan(id).ok().flatten()
            && p.inputs_hash == hash
        {
            return Ok(p);
        }
        let book = self.store.book(id)?;
        let _ = self.phonemizer.take_oov();
        let plan = plan::build_plan(
            &book,
            &project,
            &settings,
            self.phonemizer.as_ref(),
            progress,
        )?;
        self.store.save_plan(id, &plan)?;
        Ok(plan)
    }

    fn models_ready(&self) -> bool {
        Settings::load(&self.data)
            .map(|s| models::status(&self.data).ready(s.model))
            .unwrap_or(false)
    }

    fn scheduler(self: Arc<Self>) {
        loop {
            let (id, cancel) = {
                let mut st = lock(&self.state);
                loop {
                    if let Some(id) = st.queue.items.first().cloned()
                        && !st.queue.paused
                        && self.models_ready()
                    {
                        let cancel = Arc::new(AtomicBool::new(false));
                        st.active = Some(ActiveJob {
                            id: id.clone(),
                            cancel: cancel.clone(),
                        });
                        break (id, cancel);
                    }
                    st = self
                        .cv
                        .wait_timeout(st, Duration::from_secs(2))
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
            };
            let end = panic::catch_unwind(AssertUnwindSafe(|| self.run_job(&id, &cancel)))
                .unwrap_or_else(|p| {
                    JobEnd::Failed(format!(
                        "Something went wrong while making this audiobook: {}",
                        panic_text(&*p)
                    ))
                });
            self.finish_job(&id, end);
        }
    }

    fn finish_job(&self, id: &str, end: JobEnd) {
        let title = self
            .store
            .load(id)
            .map(|p| p.meta.title)
            .unwrap_or_default();
        match &end {
            JobEnd::Done(info) => {
                self.events.finished(id, &title, &Ok(info.clone()));
            }
            JobEnd::Interrupted => self.set_status(id, ProjectStatus::Paused),
            JobEnd::Failed(msg) => {
                self.set_status(
                    id,
                    ProjectStatus::Failed {
                        message: msg.clone(),
                    },
                );
                self.events.finished(id, &title, &Err(msg.clone()));
            }
            JobEnd::DiskFull => {
                let msg = "Your disk is full. Free some space, then press Resume.".to_string();
                self.set_status(
                    id,
                    ProjectStatus::Failed {
                        message: msg.clone(),
                    },
                );
                self.events.finished(id, &title, &Err(msg));
            }
        }
        let mut st = lock(&self.state);
        match end {
            JobEnd::Done(_) | JobEnd::Failed(_) => {
                if st.queue.items.first().is_some_and(|i| i == id) {
                    st.queue.items.remove(0);
                }
            }
            JobEnd::DiskFull => st.queue.paused = true,
            JobEnd::Interrupted => {}
        }
        st.active = None;
        st.progress = None;
        self.persist_and_emit(&st);
        drop(st);
        self.cv.notify_all();
    }
}

#[cfg(test)]
mod tests;
