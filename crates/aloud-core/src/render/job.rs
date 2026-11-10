//! One render job: plan → synthesize missing segments in parallel → export.

use std::{
    collections::{HashMap, HashSet},
    panic::{self, AssertUnwindSafe},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use super::{
    ChapterProgress, JobEnd, Phase, RenderProgress, RenderService, cache, lock, panic_text,
};
use crate::{
    SAMPLE_RATE,
    error::{Error, Result},
    export,
    plan::Segment,
    project::{ProjectStatus, now_secs},
    settings::{ModelVariant, Performance, Settings},
    tts::{self, Accel, EngineOptions},
};

const PROGRESS_EVERY: Duration = Duration::from_millis(250);
const RATE_WINDOW: Duration = Duration::from_secs(5);
const ETA_MIN_ELAPSED: Duration = Duration::from_secs(15);
const ETA_MIN_ITEMS: usize = 8;
/// Audio is encoded roughly this many times faster than real time.
const EXPORT_SPEEDUP: f64 = 300.0;

struct WorkItem {
    key: String,
    phonemes: String,
    speed: f32,
    units: u64,
}

struct Stats {
    done_units: u64,
    chapter_done: Vec<u64>,
    items_done: usize,
    synth_units: u64,
    audio_samples: u64,
}

/// EWMA of newly synthesized units per second over 5 s windows.
struct Meter {
    window_start: Instant,
    window_units: u64,
    ewma: Option<f64>,
}

impl Meter {
    fn update(&mut self, synth_units: u64) {
        let dt = self.window_start.elapsed();
        if dt >= RATE_WINDOW {
            let rate = (synth_units - self.window_units) as f64 / dt.as_secs_f64();
            self.ewma = Some(match self.ewma {
                Some(prev) => 0.3 * rate + 0.7 * prev,
                None => rate,
            });
            self.window_start = Instant::now();
            self.window_units = synth_units;
        }
    }
}

/// `(workers, intra-op threads per worker)` for the current settings.
pub(crate) fn worker_plan(settings: &Settings, gpu: bool, items: usize) -> (usize, usize) {
    if gpu {
        return (1, 2);
    }
    let logical = thread::available_parallelism().map_or(1, |n| n.get());
    let budget = match settings.performance {
        Performance::Full => logical,
        Performance::Balanced => (logical / 2).max(1),
    };
    let workers = (budget / 4).clamp(1, 4);
    (workers.min(items.max(1)), (budget / workers).max(1))
}

struct Layout {
    ids: Vec<String>,
    totals: Vec<u64>,
    total_units: u64,
    pending_units: u64,
}

impl Layout {
    fn progress(
        &self,
        project_id: &str,
        phase: Phase,
        stats: &Stats,
        eta: Option<u64>,
        rtf: Option<f32>,
    ) -> RenderProgress {
        RenderProgress {
            project_id: project_id.to_string(),
            phase,
            done_units: stats.done_units,
            total_units: self.total_units,
            fraction: if self.total_units == 0 {
                1.0
            } else {
                (stats.done_units as f64 / self.total_units as f64).min(1.0) as f32
            },
            chapters: self
                .ids
                .iter()
                .zip(&self.totals)
                .zip(&stats.chapter_done)
                .map(|((id, total), done)| ChapterProgress {
                    id: id.clone(),
                    done_units: *done,
                    total_units: *total,
                })
                .collect(),
            eta_secs: eta,
            realtime_factor: rtf,
        }
    }
}

impl RenderService {
    pub(super) fn run_job(&self, id: &str, cancel: &AtomicBool) -> JobEnd {
        let r = self.try_job(id, cancel);
        if let Err(e) = &r
            && !matches!(e, Error::Cancelled)
        {
            tracing::warn!("render job {id} failed: {e:?}");
        }
        match r {
            Ok(end) => end,
            Err(Error::Cancelled) => JobEnd::Interrupted,
            Err(e) if e.is_storage_full() => JobEnd::DiskFull,
            Err(Error::Invalid(m)) => JobEnd::Failed(m),
            Err(e) => JobEnd::Failed(format!(
                "Something went wrong while making this audiobook: {e}"
            )),
        }
    }

    fn try_job(&self, id: &str, cancel: &AtomicBool) -> Result<JobEnd> {
        #[cfg(test)]
        if id == super::tests::PANIC_PROJECT {
            panic!("injected test panic");
        }
        let settings = Settings::load(&self.data)?;
        let project = self.store.update(id, |p| {
            p.status = ProjectStatus::Rendering;
            Ok::<_, Error>(true)
        })?;
        self.events.project_changed(id);

        let _awake = settings
            .keep_awake
            .then(|| {
                keepawake::Builder::default()
                    .idle(true)
                    .reason("Making an audiobook")
                    .app_name("Aloud")
                    .app_reverse_domain("io.github.suft.aloud")
                    .create()
                    .ok()
            })
            .flatten();

        let included = project.included_ids();
        if included.is_empty() {
            return Err(Error::Invalid("Select at least one chapter first.".into()));
        }
        export::check_output_dir(&settings.output_dir)?;

        // --- planning ---
        let planning = |done: usize, total: usize| RenderProgress {
            project_id: id.to_string(),
            phase: Phase::Planning,
            done_units: done as u64,
            total_units: total as u64,
            fraction: if total == 0 {
                0.0
            } else {
                done as f32 / total as f32
            },
            chapters: Vec::new(),
            eta_secs: None,
            realtime_factor: None,
        };
        self.set_progress(planning(0, 1));
        let plan = self.ensure_plan(id, &|d, t| self.set_progress(planning(d, t)), cancel)?;

        // --- work list ---
        let chapters = plan.sequence(&included);
        let audio_dir = self.data.project_audio(id);
        let mut layout = Layout {
            ids: chapters.iter().map(|c| c.id.to_string()).collect(),
            totals: vec![0; chapters.len()],
            total_units: 0,
            pending_units: 0,
        };
        let mut stats = Stats {
            done_units: 0,
            chapter_done: vec![0; chapters.len()],
            items_done: 0,
            synth_units: 0,
            audio_samples: 0,
        };
        let mut seen: HashSet<&str> = HashSet::new();
        let mut cached: HashMap<&str, bool> = HashMap::new();
        let mut occurrences: HashMap<String, Vec<(usize, u64)>> = HashMap::new();
        let mut items: Vec<WorkItem> = Vec::new();
        for (ci, ch) in chapters.iter().enumerate() {
            for seg in &ch.segments {
                let Segment::Speech {
                    key,
                    phonemes,
                    speed,
                    units,
                } = seg
                else {
                    continue;
                };
                let u = *units as u64;
                layout.totals[ci] += u;
                layout.total_units += u;
                let hit = *cached
                    .entry(key.as_str())
                    .or_insert_with(|| cache::exists(&audio_dir, key));
                if hit {
                    stats.done_units += u;
                    stats.chapter_done[ci] += u;
                } else {
                    occurrences.entry(key.clone()).or_default().push((ci, u));
                    if seen.insert(key.as_str()) {
                        layout.pending_units += u;
                        items.push(WorkItem {
                            key: key.clone(),
                            phonemes: phonemes.clone(),
                            speed: *speed,
                            units: u,
                        });
                    }
                }
            }
        }

        // --- synthesis ---
        if !items.is_empty() {
            self.set_progress(layout.progress(id, Phase::Rendering, &stats, None, None));
            if let Some(end) = self.synthesize(
                id,
                &settings,
                &project.voice,
                cancel,
                &items,
                &occurrences,
                &layout,
                stats,
                &audio_dir,
            )? {
                return Ok(end);
            }
        }
        drop(chapters);

        // --- export ---
        if cancel.load(Ordering::SeqCst) {
            return Ok(JobEnd::Interrupted);
        }
        self.set_status(id, ProjectStatus::Exporting);
        let project = self.store.load(id)?;
        let finished = Stats {
            done_units: layout.total_units,
            chapter_done: layout.totals.clone(),
            items_done: 0,
            synth_units: 0,
            audio_samples: 0,
        };
        let export_progress = |f: f32| {
            let mut p = layout.progress(id, Phase::Exporting, &finished, None, None);
            p.fraction = f;
            self.set_progress(p);
        };
        export_progress(0.0);
        let last = Mutex::new(Instant::now());
        let info = export::export(
            &self.data,
            &project,
            &plan,
            &settings,
            &|f| {
                let mut t = lock(&last);
                if t.elapsed() >= PROGRESS_EVERY {
                    *t = Instant::now();
                    export_progress(f);
                }
            },
            cancel,
        )?;

        self.store.update(id, |p| {
            p.status = ProjectStatus::Done {
                output: info.output.clone(),
                finished_at: now_secs(),
                inputs_hash: p.export_signature(&plan.inputs_hash),
            };
            Ok::<_, Error>(true)
        })?;
        let removed = cache::prune(&audio_dir, &plan.speech_keys());
        if removed > 0 {
            tracing::info!("removed {removed} unused audio parts of {id}");
        }
        self.events.project_changed(id);
        Ok(JobEnd::Done(info))
    }

    /// Runs the workers. `Ok(Some(end))` means the job stopped early (paused or cancelled).
    #[allow(clippy::too_many_arguments)]
    fn synthesize(
        &self,
        id: &str,
        settings: &Settings,
        voice: &str,
        cancel: &AtomicBool,
        items: &[WorkItem],
        occurrences: &HashMap<String, Vec<(usize, u64)>>,
        layout: &Layout,
        initial: Stats,
        audio_dir: &std::path::Path,
    ) -> Result<Option<JobEnd>> {
        let gpu = settings.gpu && settings.model == ModelVariant::Standard && tts::gpu_supported();
        let accel = if gpu { Accel::Gpu } else { Accel::Cpu };
        let (workers, intra) = worker_plan(settings, gpu, items.len());

        let stats = Mutex::new(initial);
        let next = AtomicUsize::new(0);
        let fail = AtomicBool::new(false);
        let error: Mutex<Option<Error>> = Mutex::new(None);
        let gpu_notice = AtomicBool::new(false);
        let started = Instant::now();

        let work = || -> Result<()> {
            let (mut engine, fell_back) = (self.open_engine)(
                self,
                settings.model,
                EngineOptions {
                    intra_threads: intra,
                    accel,
                },
            )?;
            if fell_back && !gpu_notice.swap(true, Ordering::SeqCst) {
                self.events.notice("gpu-fallback");
            }
            loop {
                if cancel.load(Ordering::Relaxed) || fail.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(i) else {
                    return Ok(());
                };
                let audio = match engine.synth(&item.phonemes, voice, item.speed) {
                    Ok(a) => a,
                    Err(_) => engine.synth(&item.phonemes, voice, item.speed)?,
                };
                // An empty file would read as a failed write; store one silent sample instead.
                let audio = if audio.is_empty() { vec![0.0] } else { audio };
                cache::write_pcm(audio_dir, &item.key, &audio)?;
                let mut st = lock(&stats);
                for &(ci, u) in occurrences.get(&item.key).into_iter().flatten() {
                    st.chapter_done[ci] += u;
                    st.done_units += u;
                }
                st.items_done += 1;
                st.synth_units += item.units;
                st.audio_samples += audio.len() as u64;
            }
        };

        thread::scope(|s| {
            let handles: Vec<_> = (0..workers)
                .map(|_| {
                    s.spawn(|| {
                        let result = panic::catch_unwind(AssertUnwindSafe(&work))
                            .unwrap_or_else(|p| Err(Error::other(panic_text(&*p))));
                        if let Err(e) = result {
                            fail.store(true, Ordering::SeqCst);
                            lock(&error).get_or_insert(e);
                        }
                    })
                })
                .collect();

            let mut meter = Meter {
                window_start: Instant::now(),
                window_units: 0,
                ewma: None,
            };
            let mut last_emit = Instant::now() - PROGRESS_EVERY;
            loop {
                let finished = handles.iter().all(|h| h.is_finished());
                if finished || last_emit.elapsed() >= PROGRESS_EVERY {
                    last_emit = Instant::now();
                    let st = lock(&stats);
                    meter.update(st.synth_units);
                    let elapsed = started.elapsed();
                    let audio_secs = st.audio_samples as f64 / SAMPLE_RATE as f64;
                    let rtf = (elapsed.as_secs_f64() > 0.0 && st.audio_samples > 0)
                        .then(|| (audio_secs / elapsed.as_secs_f64()) as f32);
                    let eta = match meter.ewma {
                        Some(rate)
                            if rate > 0.0
                                && elapsed >= ETA_MIN_ELAPSED
                                && st.items_done >= ETA_MIN_ITEMS =>
                        {
                            let remaining = layout.pending_units.saturating_sub(st.synth_units);
                            let secs_per_unit = audio_secs / st.synth_units.max(1) as f64;
                            let export = layout.total_units as f64 * secs_per_unit / EXPORT_SPEEDUP;
                            Some((remaining as f64 / rate + export).ceil() as u64)
                        }
                        _ => None,
                    };
                    self.set_progress(layout.progress(id, Phase::Rendering, &st, eta, rtf));
                }
                if finished {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        });

        if let Some(e) = lock(&error).take() {
            return Err(e);
        }
        if cancel.load(Ordering::SeqCst) {
            return Ok(Some(JobEnd::Interrupted));
        }
        Ok(None)
    }
}
