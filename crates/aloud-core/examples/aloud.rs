//! Dev/verification CLI: imports an EPUB and renders it through the same `RenderService` the app uses.

use aloud_core::{
    DataDir, SAMPLE_RATE,
    book::{Block, ChapterKind},
    epub,
    g2p::{Dialect, Phonemize, Phonemizer},
    models,
    plan::{self, MAX_BATCH_PHONEMES, split_phonemes},
    project::ProjectStore,
    pron::{PronEntry, SayAs},
    queue::QueueState,
    render::{Events, FinishedInfo, RenderProgress, RenderService},
    settings::{ModelVariant, Settings},
    tts::{Accel, Engine, EngineOptions},
    voices::Voices,
};
use clap::Parser;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    data: PathBuf,
    #[arg(long)]
    epub: Option<PathBuf>,
    /// Print chapters (1-based) and exit.
    #[arg(long)]
    list: bool,
    /// Chapters to include, e.g. "1-3,5".
    #[arg(long)]
    chapters: Option<String>,
    #[arg(long, default_value = "af_heart")]
    voice: String,
    #[arg(long)]
    speed: Option<f32>,
    #[arg(long, default_value = "standard")]
    model: String,
    /// "word=sounds like" (book scope); repeatable.
    #[arg(long = "pron")]
    prons: Vec<String>,
    #[arg(long)]
    gpu: bool,
    #[arg(long)]
    out: Option<PathBuf>,
    /// Time synthesis of the first 2,000 characters with each available model.
    #[arg(long)]
    bench: bool,
}

struct Print {
    done: Mutex<mpsc::Sender<Result<FinishedInfo, String>>>,
    last: Mutex<Instant>,
}

fn fmt_eta(secs: u64) -> String {
    format!("{}m{:02}s", secs / 60, secs % 60)
}

impl Events for Print {
    fn progress(&self, p: &RenderProgress) {
        let mut last = self.last.lock().unwrap();
        if last.elapsed().as_millis() < 1000 {
            return;
        }
        *last = Instant::now();
        let eta = p.eta_secs.map_or("--".into(), fmt_eta);
        let rtf = p
            .realtime_factor
            .map_or(String::new(), |r| format!(" {r:.1}x"));
        println!("[{:?}] {:.1}% eta {eta}{rtf}", p.phase, p.fraction * 100.0);
    }
    fn project_changed(&self, _: &str) {}
    fn queue_changed(&self, _: &QueueState) {}
    fn finished(&self, _: &str, _: &str, result: &Result<FinishedInfo, String>) {
        let _ = self.done.lock().unwrap().send(result.clone());
    }
    fn notice(&self, code: &str) {
        println!("notice: {code}");
    }
}

fn parse_chapters(spec: &str, n: usize) -> Vec<bool> {
    let mut on = vec![false; n];
    for part in spec.split(',').filter(|p| !p.trim().is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (
                a.trim().parse::<usize>().unwrap(),
                b.trim().parse::<usize>().unwrap(),
            ),
            None => {
                let v = part.trim().parse::<usize>().unwrap();
                (v, v)
            }
        };
        for i in a..=b {
            if (1..=n).contains(&i) {
                on[i - 1] = true;
            }
        }
    }
    on
}

fn bench(data: &DataDir, svc: &RenderService, id: &str) {
    let book = svc.store().book(id).unwrap();
    let text: String = book
        .chapters
        .iter()
        .find(|c| c.kind == ChapterKind::Body)
        .map(|c| {
            c.blocks
                .iter()
                .filter_map(|b| match b {
                    Block::Para(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
        .chars()
        .take(2000)
        .collect();
    let ph = Phonemizer::new()
        .phonemize(Dialect::Us, &text)
        .expect("phonemize");
    let batches = split_phonemes(&plan::only_vocab(&ph), MAX_BATCH_PHONEMES);
    let voices = Arc::new(Voices::load(&models::voices_path(data)).expect("voices"));
    let status = models::status(data);
    for (variant, present) in [
        (ModelVariant::Standard, status.standard),
        (ModelVariant::Fast, status.fast),
    ] {
        if !present {
            continue;
        }
        let mut engine = Engine::new(
            &models::model_path(data, variant),
            voices.clone(),
            EngineOptions {
                intra_threads: 4,
                accel: Accel::Cpu,
            },
        )
        .expect("engine");
        let t0 = Instant::now();
        let mut samples = 0usize;
        for b in &batches {
            samples += engine.synth(b, "af_heart", 1.0).expect("synth").len();
        }
        let secs = t0.elapsed().as_secs_f64();
        let audio = samples as f64 / SAMPLE_RATE as f64;
        println!(
            "model={} secs={secs:.2} audio={audio:.2} rtf={:.3}",
            variant.as_str(),
            secs / audio
        );
    }
}

fn main() {
    let args = Args::parse();
    if std::env::var_os("PIPER_ESPEAKNG_DATA_DIRECTORY").is_none() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/resources");
        // SAFETY: runs first in main, before any other thread exists.
        unsafe { std::env::set_var("PIPER_ESPEAKNG_DATA_DIRECTORY", dir) };
    }

    let data = DataDir::new(&args.data);
    let mut settings = Settings::load(&data).unwrap();
    settings.model = match args.model.as_str() {
        "fast" => ModelVariant::Fast,
        _ => ModelVariant::Standard,
    };
    settings.gpu = args.gpu;
    if let Some(out) = &args.out {
        settings.output_dir = out.clone();
    }
    settings.output_dir = std::path::absolute(&settings.output_dir).unwrap();
    settings.save(&data).unwrap();

    let epub_path = args.epub.as_ref().expect("--epub is required");
    let summary = epub::import(&data, epub_path).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let store = ProjectStore::new(data.clone());
    let mut project = store.load(&summary.id).unwrap();

    if args.list {
        let book = store.book(&summary.id).unwrap();
        for (i, c) in book.chapters.iter().enumerate() {
            println!(
                "{:>3}  {:<11} {:>6} words  {}",
                i + 1,
                format!("{:?}", c.kind),
                c.words,
                c.title
            );
        }
        return;
    }

    if let Some(spec) = &args.chapters {
        let on = parse_chapters(spec, project.chapters.len());
        for (c, on) in project.chapters.iter_mut().zip(on) {
            c.included = on;
        }
    }
    project.voice = args.voice.clone();
    if let Some(s) = args.speed {
        project.speed = s;
    }
    for p in &args.prons {
        if let Some((word, say)) = p.split_once('=') {
            project
                .pronunciations
                .retain(|e| !e.word.eq_ignore_ascii_case(word));
            let e = PronEntry::new(word, SayAs::Text(say.to_string()), false);
            e.validate().unwrap();
            project.pronunciations.push(e);
        }
    }
    store.save(&mut project).unwrap();

    let (tx, rx) = mpsc::channel();
    let svc = RenderService::new(
        data.clone(),
        Arc::new(Print {
            done: Mutex::new(tx),
            last: Mutex::new(Instant::now()),
        }),
    );

    if args.bench {
        bench(&data, &svc, &summary.id);
        return;
    }

    {
        let svc = svc.clone();
        ctrlc::set_handler(move || {
            svc.pause();
            std::process::exit(130);
        })
        .unwrap();
    }

    // Report how much of the work is already cached (the resume state).
    let plan = svc.ensure_plan(&summary.id, &|_, _| {}).unwrap();
    let included = project.included_ids();
    let audio = data.project_audio(&summary.id);
    let (mut done, mut total) = (0u64, 0u64);
    for ch in plan.sequence(&included) {
        for seg in ch.segments {
            if let plan::Segment::Speech { key, units, .. } = seg {
                total += *units as u64;
                if aloud_core::render::cache::exists(&audio, key) {
                    done += *units as u64;
                }
            }
        }
    }
    println!("cached {done}/{total} units");

    svc.enqueue(&summary.id).unwrap();
    svc.resume();
    match rx.recv().unwrap() {
        Ok(info) => println!("{} ({} s)", info.output.display(), info.duration_secs),
        Err(e) => {
            eprintln!("failed: {e}");
            std::process::exit(1);
        }
    }
}
