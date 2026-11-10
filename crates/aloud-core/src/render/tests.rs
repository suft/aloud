use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use super::*;
use crate::{
    book::{Book, BookMeta},
    models,
    project::Project,
    settings::ModelVariant,
};

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

    // `models::status` only checks sizes, so sparse files make the models look installed.
    std::fs::create_dir_all(data.models()).unwrap();
    for (path, size) in [
        (
            models::model_path(&data, ModelVariant::Fast),
            models::FAST.size,
        ),
        (models::voices_path(&data), models::VOICES.size),
    ] {
        std::fs::File::create(path).unwrap().set_len(size).unwrap();
    }

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
