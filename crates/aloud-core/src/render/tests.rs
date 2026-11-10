use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use super::*;
use crate::{
    book::{Block, Book, BookMeta, Chapter as BookChapter, ChapterKind},
    g2p::Dialect,
    models,
    plan::Segment,
    project::Project,
    settings::ModelVariant,
};

/// `models::status` only checks sizes, so sparse files make the models look installed.
fn install_models(data: &DataDir) {
    std::fs::create_dir_all(data.models()).unwrap();
    for (path, size) in [
        (
            models::model_path(data, ModelVariant::Fast),
            models::FAST.size,
        ),
        (models::voices_path(data), models::VOICES.size),
    ] {
        std::fs::File::create(path).unwrap().set_len(size).unwrap();
    }
}

/// Project id for which `try_job` panics (test-only failpoint).
pub(super) const PANIC_PROJECT: &str = "panic-project";

type Finished = (String, std::result::Result<FinishedInfo, String>);

struct Recorder(Mutex<mpsc::Sender<Finished>>);

impl Events for Recorder {
    fn progress(&self, _: &RenderProgress) {}
    fn project_changed(&self, _: &str) {}
    fn queue_changed(&self, _: &QueueState) {}
    fn finished(&self, id: &str, _: &str, result: &std::result::Result<FinishedInfo, String>) {
        let _ = lock(&self.0).send((id.to_string(), result.clone()));
    }
    fn notice(&self, _: &str) {}
}

#[test]
fn panic_in_a_job_fails_it_and_the_queue_continues() {
    let dir = tempfile::tempdir().unwrap();
    let data = DataDir::new(dir.path().join("data"));
    let settings = Settings {
        model: ModelVariant::Fast,
        keep_awake: false,
        output_dir: dir.path().join("out"),
        ..Settings::default()
    };
    settings.save(&data).unwrap();

    install_models(&data);

    // No chapters: a job that survives the failpoint fails fast with "Select at least one chapter".
    let book = Book {
        meta: BookMeta::default(),
        chapters: vec![],
    };
    let store = ProjectStore::new(data.clone());
    for id in [PANIC_PROJECT, "after-panic"] {
        let mut p = Project::new(id, "b.epub".into(), &book, &settings);
        store.save(&mut p).unwrap();
    }

    let (tx, rx) = mpsc::channel();
    let svc = RenderService::new(data.clone(), Arc::new(Recorder(Mutex::new(tx))));
    svc.enqueue(PANIC_PROJECT).unwrap();
    svc.enqueue("after-panic").unwrap();

    let wait = Duration::from_secs(20);
    let (id, result) = rx.recv_timeout(wait).expect("panicking job never finished");
    assert_eq!(id, PANIC_PROJECT);
    let msg = result.unwrap_err();
    assert!(
        msg.starts_with("Something went wrong while making this audiobook:"),
        "{msg}"
    );
    assert!(msg.contains("injected test panic"), "{msg}");

    let (id, result) = rx.recv_timeout(wait).expect("queue stalled after a panic");
    assert_eq!(id, "after-panic");
    assert_eq!(result.unwrap_err(), "Select at least one chapter first.");

    assert!(matches!(
        svc.store().load(PANIC_PROJECT).unwrap().status,
        ProjectStatus::Failed { .. }
    ));
    svc.wait_idle(Duration::from_secs(5));
    assert!(!svc.is_busy());
}

// --- scheduler tests with a fake engine ---

const TIMEOUT: Duration = Duration::from_secs(30);

/// Lowercases the text; the plan keeps only vocabulary characters, so letters and spaces survive.
struct FakePhonemizer;

impl Phonemize for FakePhonemizer {
    fn phonemize(&self, _: Dialect, text: &str) -> Result<String> {
        Ok(text.to_lowercase())
    }
    fn take_oov(&self) -> HashMap<String, u32> {
        HashMap::new()
    }
}

/// A quarter second of a 440 Hz tone (loudness normalisation K-weights away DC) after `delay`.
struct FakeSynth {
    calls: Arc<AtomicUsize>,
    delay: Duration,
}

impl Synth for FakeSynth {
    fn synth(&mut self, _: &str, _: &str, _: f32) -> Result<Vec<f32>> {
        thread::sleep(self.delay);
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok((0..6000)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 24_000.0).sin() * 0.3)
            .collect())
    }
}

/// Letters only: digits are not in the phoneme vocabulary, so they would not make paragraphs differ.
fn letters(mut i: usize) -> String {
    let mut s = String::new();
    for _ in 0..3 {
        s.push((b'a' + (i % 26) as u8) as char);
        i /= 26;
    }
    s
}

fn settings_in(dir: &std::path::Path) -> Settings {
    Settings {
        model: ModelVariant::Fast,
        keep_awake: false,
        output_dir: dir.join("out"),
        ..Settings::default()
    }
}

/// A data dir whose models look installed, holding one project per id, each with a single
/// chapter of `paragraphs` distinct paragraphs.
fn fixture(ids: &[&str], paragraphs: usize) -> (tempfile::TempDir, DataDir) {
    let dir = tempfile::tempdir().unwrap();
    let data = DataDir::new(dir.path().join("data"));
    let settings = settings_in(dir.path());
    settings.save(&data).unwrap();
    install_models(&data);

    let blocks: Vec<Block> = (0..paragraphs)
        .map(|i| Block::Para(format!("Sentence {} is a short line of text.", letters(i))))
        .collect();
    let book = Book {
        meta: BookMeta {
            title: "T".into(),
            authors: vec!["A".into()],
            ..BookMeta::default()
        },
        chapters: vec![BookChapter {
            id: "c1".into(),
            title: "One".into(),
            kind: ChapterKind::Body,
            words: BookChapter::count_words(&blocks),
            blocks,
        }],
    };
    let store = ProjectStore::new(data.clone());
    for id in ids {
        let mut p = Project::new(id, "b.epub".into(), &book, &settings);
        store.save(&mut p).unwrap();
        store.save_book(id, &book).unwrap();
    }
    (dir, data)
}

fn service(
    data: &DataDir,
    delay: Duration,
) -> (
    Arc<RenderService>,
    Arc<AtomicUsize>,
    mpsc::Receiver<Finished>,
) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let open: OpenEngine = Box::new(move |_, _, _| {
        let synth = FakeSynth {
            calls: counter.clone(),
            delay,
        };
        Ok((Box::new(synth) as Box<dyn Synth>, false))
    });
    let (tx, rx) = mpsc::channel();
    let svc = RenderService::with_backend(
        data.clone(),
        Arc::new(Recorder(Mutex::new(tx))),
        Box::new(FakePhonemizer),
        open,
    );
    (svc, calls, rx)
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn pcm_count(dir: &std::path::Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .map(|e| {
            let path = e.path();
            if path.is_dir() {
                pcm_count(&path)
            } else {
                usize::from(path.extension().is_some_and(|x| x == "pcm"))
            }
        })
        .sum()
}

/// Distinct speech segments the project's export needs.
fn unique_speech(svc: &RenderService, id: &str) -> HashSet<String> {
    let project = svc.store().load(id).unwrap();
    let plan = svc.store().load_plan(id).unwrap().expect("plan was built");
    plan.sequence(&project.included_ids())
        .iter()
        .flat_map(|c| c.segments.iter())
        .filter_map(|s| match s {
            Segment::Speech { key, .. } => Some(key.clone()),
            Segment::Silence { .. } => None,
        })
        .collect()
}

fn status_of(svc: &RenderService, id: &str) -> ProjectStatus {
    svc.store().load(id).unwrap().status
}

#[test]
fn renders_and_exports_with_a_fake_engine() {
    let (_dir, data) = fixture(&["book"], 10);
    let (svc, calls, rx) = service(&data, Duration::ZERO);
    svc.enqueue("book").unwrap();

    let (id, result) = rx.recv_timeout(TIMEOUT).expect("job never finished");
    assert_eq!(id, "book");
    let info = result.expect("render failed");
    assert!(info.output.is_file());
    svc.wait_idle(Duration::from_secs(5));
    assert!(matches!(
        status_of(&svc, "book"),
        ProjectStatus::Done { .. }
    ));
    assert!(svc.queue().items.is_empty());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        unique_speech(&svc, "book").len()
    );
}

#[test]
fn pause_keeps_finished_parts_and_resume_renders_only_the_rest() {
    let (_dir, data) = fixture(&["book"], 40);
    let (svc, calls, rx) = service(&data, Duration::from_millis(50));
    svc.enqueue("book").unwrap();
    wait_until("three utterances", || calls.load(Ordering::SeqCst) >= 3);

    let q = svc.pause();
    assert!(q.paused);
    assert_eq!(svc.queue().items, ["book"]);
    assert!(matches!(status_of(&svc, "book"), ProjectStatus::Paused));
    let unique = unique_speech(&svc, "book").len();
    let done = calls.load(Ordering::SeqCst);
    assert!(
        done < unique,
        "{done} of {unique}: finished before the pause"
    );
    assert_eq!(pcm_count(&data.project_audio("book")), done);

    svc.resume();
    let (_, result) = rx
        .recv_timeout(TIMEOUT)
        .expect("resumed job never finished");
    result.expect("render failed");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        unique,
        "something rendered twice"
    );
}

#[test]
fn cancel_stops_the_job_and_keeps_its_audio() {
    let (_dir, data) = fixture(&["book"], 40);
    let (svc, calls, rx) = service(&data, Duration::from_millis(50));
    svc.enqueue("book").unwrap();
    wait_until("three utterances", || calls.load(Ordering::SeqCst) >= 3);

    let q = svc.cancel("book");
    assert!(q.items.is_empty());
    assert!(matches!(status_of(&svc, "book"), ProjectStatus::Paused));
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    assert_eq!(
        pcm_count(&data.project_audio("book")),
        calls.load(Ordering::SeqCst)
    );
}

#[test]
fn move_item_reorders_but_never_moves_the_running_head() {
    let (_dir, data) = fixture(&["a", "b", "c"], 2);
    let (svc, _, _rx) = service(&data, Duration::ZERO);
    svc.pause();
    for id in ["a", "b", "c"] {
        svc.enqueue(id).unwrap();
    }
    assert_eq!(svc.move_item("c", Direction::Up).items, ["a", "c", "b"]);
    assert_eq!(svc.move_item("a", Direction::Down).items, ["c", "a", "b"]);
    assert_eq!(svc.move_item("c", Direction::Up).items, ["c", "a", "b"]);

    let (_dir, data) = fixture(&["a", "b", "c"], 40);
    let (svc, _, _rx) = service(&data, Duration::from_millis(200));
    for id in ["a", "b", "c"] {
        svc.enqueue(id).unwrap();
    }
    wait_until("the first job to start", || svc.active().is_some());
    assert_eq!(svc.move_item("b", Direction::Up).items, ["a", "b", "c"]);
    assert_eq!(svc.move_item("c", Direction::Up).items, ["a", "c", "b"]);
    svc.pause();
}

#[test]
fn startup_reconciles_statuses_left_by_a_crash() {
    let (dir, data) = fixture(&["a", "b", "c", "d"], 2);
    let store = ProjectStore::new(data.clone());
    let set = |id: &str, status: ProjectStatus| {
        let mut p = store.load(id).unwrap();
        p.status = status;
        store.save(&mut p).unwrap();
    };
    set("a", ProjectStatus::Rendering);
    set("b", ProjectStatus::Exporting);
    set("c", ProjectStatus::Queued);
    set(
        "d",
        ProjectStatus::Done {
            output: dir.path().join("x.m4b"),
            finished_at: 0,
            inputs_hash: "h".into(),
        },
    );
    QueueState {
        items: vec!["a".into(), "missing".into()],
        paused: false,
    }
    .save(&data)
    .unwrap();

    let (svc, calls, _rx) = service(&data, Duration::ZERO);
    let expected = QueueState {
        items: vec!["a".into()],
        paused: true,
    };
    assert_eq!(svc.queue(), expected);
    assert_eq!(QueueState::load(&data).unwrap(), expected);
    assert!(matches!(status_of(&svc, "a"), ProjectStatus::Queued));
    assert!(matches!(status_of(&svc, "b"), ProjectStatus::Paused));
    assert!(matches!(status_of(&svc, "c"), ProjectStatus::Paused));
    assert!(matches!(status_of(&svc, "d"), ProjectStatus::Done { .. }));

    thread::sleep(Duration::from_millis(300));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a paused queue must not render"
    );
}
