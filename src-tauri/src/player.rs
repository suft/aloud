//! Native playback of cached chapters and previews, on a dedicated thread that owns the audio device.

use aloud_core::{SAMPLE_RATE, render::cache};
use rodio::{ChannelCount, MixerDeviceSink, Player as Sink, SampleRate, Source};
use serde::Serialize;
use std::{
    num::NonZero,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};

const TICK: Duration = Duration::from_millis(250);

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
    Stop,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PlayerState {
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
}

fn samples_to_ms(s: u64) -> u64 {
    s * 1000 / SAMPLE_RATE as u64
}

/// Streams a chapter's segments lazily, reading each `.pcm` only when it is reached.
struct ChapterSource {
    segs: Arc<Vec<Seg>>,
    idx: usize,
    cur: Arc<[f32]>,
    pos: usize,
    silence_left: u64,
    consumed: Arc<AtomicU64>,
    total: u64,
}

impl ChapterSource {
    fn new(segs: Arc<Vec<Seg>>, start: u64, consumed: Arc<AtomicU64>) -> Self {
        let total = segs.iter().map(Seg::len).sum();
        let mut src = ChapterSource {
            segs,
            idx: 0,
            cur: Arc::from([]),
            pos: 0,
            silence_left: 0,
            consumed,
            total,
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
        self.cur = Arc::from([]);
        match &self.segs[self.idx] {
            Seg::Silence(n) => self.silence_left = n - offset,
            Seg::Mem(m) => {
                self.cur = m.clone();
                self.pos = offset as usize;
            }
            Seg::File { path, samples } => {
                // Missing files play as silence rather than ending playback.
                match std::fs::read(path) {
                    Ok(bytes) => {
                        let v: Vec<f32> = bytes
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32767.0)
                            .collect();
                        self.cur = Arc::from(v);
                        self.pos = offset as usize;
                    }
                    Err(_) => self.silence_left = samples - offset,
                }
            }
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
}

impl Loaded {
    fn position_ms(&self) -> u64 {
        samples_to_ms((self.start + self.consumed.load(Ordering::Relaxed)).min(self.total))
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
            playing: !self.finished && !self.sink.is_paused(),
        }
    }
}

fn run(app: AppHandle, rx: mpsc::Receiver<Cmd>) {
    let mut device: Option<MixerDeviceSink> = None;
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

    let start = |device: &mut Option<MixerDeviceSink>,
                 project_id: Option<String>,
                 chapter_id: Option<String>,
                 title: String,
                 segs: Arc<Vec<Seg>>,
                 start_samples: u64|
     -> Option<Loaded> {
        if device.is_none() {
            match rodio::DeviceSinkBuilder::open_default_sink() {
                Ok(d) => *device = Some(d),
                Err(_) => {
                    let _ = app.emit("notice", serde_json::json!({ "code": "no-audio-device" }));
                    return None;
                }
            }
        }
        let sink = Sink::connect_new(device.as_ref()?.mixer());
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
                    let was_paused = l.sink.is_paused() && !l.finished;
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
                emit(&loaded);
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(l) = &mut loaded {
                    if !l.finished && l.sink.empty() {
                        l.finished = true;
                        emit(&loaded);
                    } else if !l.finished && !l.sink.is_paused() {
                        emit(&loaded);
                    }
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
