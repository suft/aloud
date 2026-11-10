//! Native playback of cached chapters and previews, on a dedicated thread that owns the audio device.

use aloud_core::{SAMPLE_RATE, render::cache};
use rodio::{ChannelCount, MixerDeviceSink, Player as Sink, SampleRate, Source};
use serde::Serialize;
use std::{
    num::NonZero,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender},
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};

const TICK: Duration = Duration::from_millis(250);
/// Consecutive ticks (2 s) without the output consuming samples before playback counts as stalled.
const STALL_TICKS: u32 = 8;

/// One piece of a chapter's audio, in playback order.
#[derive(Debug, Clone)]
pub enum Seg {
    Silence(u64),
    File { path: PathBuf, samples: u64 },
    Mem(Arc<[f32]>),
}

impl Seg {
    fn len(&self) -> u64 {
        match self {
            Seg::Silence(n) => *n,
            Seg::File { samples, .. } => *samples,
            Seg::Mem(m) => m.len() as u64,
        }
    }
}

pub enum Cmd {
    Play {
        project_id: Option<String>,
        chapter_id: Option<String>,
        title: String,
        segs: Vec<Seg>,
        start_ms: u64,
    },
    Toggle,
    Seek(u64),
    StopProject(String),
    Stop,
}

#[derive(ts_rs::TS, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlayerState {
    project_id: Option<String>,
    chapter_id: Option<String>,
    title: String,
    position_ms: u64,
    duration_ms: u64,
    playing: bool,
}

#[derive(Clone)]
pub struct Player {
    tx: Sender<Cmd>,
}

impl Player {
    pub fn spawn(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("aloud-player".into())
            .spawn(move || run(app, rx))
            .expect("spawn player thread");
        Player { tx }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    /// Stops playback only if the loaded chapter belongs to project `id`.
    pub fn stop_project(&self, id: &str) {
        self.send(Cmd::StopProject(id.to_string()));
    }
}

fn samples_to_ms(s: u64) -> u64 {
    s * 1000 / SAMPLE_RATE as u64
}

/// Streams a chapter's segments lazily, reading each `.pcm` only when it is reached. The next
/// file segment is read ahead on a helper thread, so the audio thread rarely waits on disk.
struct ChapterSource {
    segs: Arc<Vec<Seg>>,
    idx: usize,
    cur: Arc<[f32]>,
    pos: usize,
    silence_left: u64,
    consumed: Arc<AtomicU64>,
    total: u64,
    /// Requests to the prefetch thread; `None` if it couldn't be started.
    want: Option<SyncSender<usize>>,
    got: Receiver<(usize, Option<Arc<[f32]>>)>,
    /// Index of the segment the prefetch thread is reading, until it is taken by `load`.
    pending: Option<usize>,
    empty: Arc<[f32]>,
}

fn read_seg(path: &Path) -> Option<Arc<[f32]>> {
    cache::read_pcm_file(path).ok().map(Arc::from)
}

impl ChapterSource {
    fn new(segs: Arc<Vec<Seg>>, start: u64, consumed: Arc<AtomicU64>) -> Self {
        let total = segs.iter().map(Seg::len).sum();
        let (want_tx, want_rx) = mpsc::sync_channel::<usize>(1);
        let (got_tx, got_rx) = mpsc::sync_channel(1);
        let reader_segs = segs.clone();
        let want = std::thread::Builder::new()
            .name("aloud-prefetch".into())
            .spawn(move || {
                for i in want_rx {
                    let data = match &reader_segs[i] {
                        Seg::File { path, .. } => read_seg(path),
                        _ => None,
                    };
                    if got_tx.send((i, data)).is_err() {
                        break;
                    }
                }
            })
            .map(|_| want_tx)
            .map_err(|e| log::warn!("couldn't start the prefetch thread: {e}"))
            .ok();
        let mut src = ChapterSource {
            segs,
            idx: 0,
            cur: Arc::from([]),
            pos: 0,
            silence_left: 0,
            consumed,
            total,
            want,
            got: got_rx,
            pending: None,
            empty: Arc::from([]),
        };
        let mut offset = start;
        while src.idx < src.segs.len() && offset >= src.segs[src.idx].len() {
            offset -= src.segs[src.idx].len();
            src.idx += 1;
        }
        if src.idx < src.segs.len() {
            src.load(offset);
        }
        src
    }

    fn load(&mut self, offset: u64) {
        self.pos = 0;
        self.silence_left = 0;
        self.cur = self.empty.clone();
        match &self.segs[self.idx] {
            Seg::Silence(n) => self.silence_left = n - offset,
            Seg::Mem(m) => {
                self.cur = m.clone();
                self.pos = offset as usize;
            }
            Seg::File { path, samples } => {
                let data = match self.pending.take_if(|p| *p == self.idx) {
                    // Blocks only if the read is still running.
                    Some(_) => match self.got.recv() {
                        Ok((_, data)) => data,
                        Err(_) => read_seg(path),
                    },
                    // Only the first segment, which `new` loads on the player thread.
                    None => read_seg(path),
                };
                // Missing files play as silence rather than ending playback.
                match data {
                    Some(d) => {
                        self.cur = d;
                        self.pos = offset as usize;
                    }
                    None => self.silence_left = samples - offset,
                }
            }
        }
        self.prefetch_next();
    }

    fn prefetch_next(&mut self) {
        let Some(want) = &self.want else { return };
        if self.pending.is_some() {
            return;
        }
        let next =
            (self.idx + 1..self.segs.len()).find(|&j| matches!(self.segs[j], Seg::File { .. }));
        if let Some(j) = next
            && want.try_send(j).is_ok()
        {
            self.pending = Some(j);
        }
    }
}

impl Iterator for ChapterSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        loop {
            if self.silence_left > 0 {
                self.silence_left -= 1;
                self.consumed.fetch_add(1, Ordering::Relaxed);
                return Some(0.0);
            }
            if self.pos < self.cur.len() {
                let v = self.cur[self.pos];
                self.pos += 1;
                self.consumed.fetch_add(1, Ordering::Relaxed);
                return Some(v);
            }
            self.idx += 1;
            if self.idx >= self.segs.len() {
                return None;
            }
            self.load(0);
        }
    }
}

impl Source for ChapterSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        NonZero::new(1).expect("nonzero")
    }
    fn sample_rate(&self) -> SampleRate {
        NonZero::new(SAMPLE_RATE).expect("nonzero")
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.total as f64 / SAMPLE_RATE as f64,
        ))
    }
}

/// An open output stream, plus whether the backend has reported that the device went away.
struct Output {
    sink: MixerDeviceSink,
    lost: Arc<AtomicBool>,
}

fn device_gone(kind: rodio::CpalErrorKind) -> bool {
    use rodio::CpalErrorKind::*;
    matches!(
        kind,
        DeviceChanged
            | DeviceNotAvailable
            | HostUnavailable
            | StreamInvalidated
            | BackendError
            | Other
    )
}

fn open_output() -> Option<Output> {
    let lost = Arc::new(AtomicBool::new(false));
    let flag = lost.clone();
    let on_error = move |e: rodio::CpalError| {
        log::warn!("audio output error: {e}");
        if device_gone(e.kind()) {
            flag.store(true, Ordering::SeqCst);
        }
    };
    // The fallback keeps rodio's search for a non-default device; only the stall watchdog covers it.
    let sink = rodio::DeviceSinkBuilder::from_default_device()
        .and_then(|b| b.with_error_callback(on_error).open_stream())
        .or_else(|_| rodio::DeviceSinkBuilder::open_default_sink())
        .ok()?;
    Some(Output { sink, lost })
}

struct Loaded {
    project_id: Option<String>,
    chapter_id: Option<String>,
    title: String,
    segs: Arc<Vec<Seg>>,
    total: u64,
    start: u64,
    consumed: Arc<AtomicU64>,
    sink: Sink,
    finished: bool,
    /// The audio device went away; the sink is closed and `Toggle` restarts at the current position.
    orphaned: bool,
    last_consumed: u64,
    stalled: u32,
}

impl Loaded {
    fn position_ms(&self) -> u64 {
        samples_to_ms((self.start + self.consumed.load(Ordering::Relaxed)).min(self.total))
    }

    fn playing(&self) -> bool {
        !self.finished && !self.orphaned && !self.sink.is_paused()
    }

    fn state(&self) -> PlayerState {
        PlayerState {
            project_id: self.project_id.clone(),
            chapter_id: self.chapter_id.clone(),
            title: self.title.clone(),
            position_ms: if self.finished {
                samples_to_ms(self.total)
            } else {
                self.position_ms()
            },
            duration_ms: samples_to_ms(self.total),
            playing: self.playing(),
        }
    }
}

fn run(app: AppHandle, rx: mpsc::Receiver<Cmd>) {
    let mut device: Option<Output> = None;
    let mut loaded: Option<Loaded> = None;
    let emit = |l: &Option<Loaded>| {
        let state = match l {
            Some(l) => l.state(),
            None => PlayerState {
                project_id: None,
                chapter_id: None,
                title: String::new(),
                position_ms: 0,
                duration_ms: 0,
                playing: false,
            },
        };
        let _ = app.emit("player-state", state);
    };

    let start = |device: &mut Option<Output>,
                 project_id: Option<String>,
                 chapter_id: Option<String>,
                 title: String,
                 segs: Arc<Vec<Seg>>,
                 start_samples: u64|
     -> Option<Loaded> {
        if device
            .as_ref()
            .is_some_and(|d| d.lost.load(Ordering::SeqCst))
        {
            *device = None;
        }
        if device.is_none() {
            *device = open_output();
            if device.is_none() {
                let _ = app.emit("notice", serde_json::json!({ "code": "no-audio-device" }));
                return None;
            }
        }
        let sink = Sink::connect_new(device.as_ref()?.sink.mixer());
        let consumed = Arc::new(AtomicU64::new(0));
        let source = ChapterSource::new(segs.clone(), start_samples, consumed.clone());
        let total = source.total;
        sink.append(source);
        Some(Loaded {
            project_id,
            chapter_id,
            title,
            segs,
            total,
            start: start_samples,
            consumed,
            sink,
            finished: false,
            orphaned: false,
            last_consumed: 0,
            stalled: 0,
        })
    };

    loop {
        match rx.recv_timeout(TICK) {
            Ok(Cmd::Play {
                project_id,
                chapter_id,
                title,
                segs,
                start_ms,
            }) => {
                drop(loaded.take());
                let start_samples = start_ms * SAMPLE_RATE as u64 / 1000;
                loaded = start(
                    &mut device,
                    project_id,
                    chapter_id,
                    title,
                    Arc::new(segs),
                    start_samples,
                );
                emit(&loaded);
            }
            Ok(Cmd::Toggle) => {
                if let Some(l) = loaded.take() {
                    if l.finished {
                        loaded = start(&mut device, l.project_id, l.chapter_id, l.title, l.segs, 0);
                    } else if l.orphaned {
                        let at = (l.start + l.consumed.load(Ordering::Relaxed)).min(l.total);
                        loaded =
                            start(&mut device, l.project_id, l.chapter_id, l.title, l.segs, at);
                    } else {
                        if l.sink.is_paused() {
                            l.sink.play();
                        } else {
                            l.sink.pause();
                        }
                        loaded = Some(l);
                    }
                    emit(&loaded);
                }
            }
            Ok(Cmd::Seek(ms)) => {
                if let Some(l) = loaded.take() {
                    let was_paused = (l.sink.is_paused() || l.orphaned) && !l.finished;
                    let target = (ms * SAMPLE_RATE as u64 / 1000).min(l.total);
                    loaded = start(
                        &mut device,
                        l.project_id,
                        l.chapter_id,
                        l.title,
                        l.segs,
                        target,
                    );
                    if was_paused && let Some(l) = &loaded {
                        l.sink.pause();
                    }
                    emit(&loaded);
                }
            }
            Ok(Cmd::Stop) => {
                loaded = None;
                device = None;
                emit(&loaded);
            }
            Ok(Cmd::StopProject(id)) => {
                if loaded
                    .as_ref()
                    .is_some_and(|l| l.project_id.as_deref() == Some(id.as_str()))
                {
                    loaded = None;
                    device = None;
                    emit(&loaded);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                let lost = device
                    .as_ref()
                    .is_some_and(|d| d.lost.load(Ordering::SeqCst));
                let mut notify = false;
                if let Some(l) = &mut loaded {
                    if !l.finished && !l.orphaned && l.sink.empty() {
                        l.finished = true;
                        device = None;
                        emit(&loaded);
                    } else if l.playing() {
                        let consumed = l.consumed.load(Ordering::Relaxed);
                        if consumed == l.last_consumed {
                            l.stalled += 1;
                        } else {
                            l.stalled = 0;
                            l.last_consumed = consumed;
                        }
                        if lost || l.stalled >= STALL_TICKS {
                            // The device is gone or silently stopped pulling samples: pause so the
                            // position stops advancing; play reopens the output where we left off.
                            l.sink.pause();
                            l.orphaned = true;
                            device = None;
                            notify = true;
                        }
                        emit(&loaded);
                    } else {
                        l.stalled = 0;
                    }
                } else if lost {
                    device = None;
                }
                if notify {
                    let _ = app.emit("notice", serde_json::json!({ "code": "audio-device-lost" }));
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Builds the segment list for cached audio files.
pub fn file_seg(audio_dir: &std::path::Path, key: &str) -> Option<Seg> {
    let samples = cache::len_samples(audio_dir, key).ok()?;
    Some(Seg::File {
        path: cache::pcm_path(audio_dir, key),
        samples,
    })
}
