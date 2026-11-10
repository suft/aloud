use super::*;
use crate::{
    book::{BookMeta, Chapter},
    pron::{PronEntry, SayAs},
};
use std::sync::{Mutex, atomic::AtomicBool};

static NEVER: AtomicBool = AtomicBool::new(false);

/// Stand-in for espeak/misaki: lowercases the text and records words starting with 'z' as unknown.
#[derive(Default)]
struct Fake(Mutex<HashMap<String, u32>>);

impl Phonemize for Fake {
    fn phonemize(&self, _: Dialect, text: &str) -> Result<String> {
        for w in text.split_whitespace() {
            let w = w.trim_matches(|c: char| !c.is_alphabetic()).to_lowercase();
            if w.starts_with('z') {
                *self.0.lock().unwrap().entry(w).or_insert(0) += 1;
            }
        }
        Ok(text.to_lowercase())
    }
    fn take_oov(&self) -> HashMap<String, u32> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

fn book(paras: &[&str]) -> Book {
    let blocks: Vec<Block> = paras.iter().map(|p| Block::Para(p.to_string())).collect();
    Book {
        meta: BookMeta {
            title: "T".into(),
            authors: vec!["A".into()],
            ..Default::default()
        },
        chapters: vec![Chapter {
            id: "c1".into(),
            title: "One".into(),
            kind: ChapterKind::Body,
            words: Chapter::count_words(&blocks),
            blocks,
        }],
    }
}

fn speech_keys(plan: &Plan) -> Vec<String> {
    plan.chapters[0]
        .segments
        .iter()
        .filter_map(|s| match s {
            Segment::Speech { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn split_respects_limit_and_sentence_boundaries() {
    let sentence = "abcdefghij klmnopqrst uvwxyzabcd efghijklmn opqrstuvwx yzabcdefgh ijklmnopqr.";
    let text = [sentence; 12].join(" ");
    assert!(text.chars().count() > 900);
    let batches = split_phonemes(&text, 300);
    assert!(batches.iter().all(|b| b.chars().count() <= 300));
    assert!(batches.iter().all(|b| b.ends_with('.')), "{batches:?}");
    let lens: Vec<usize> = batches.iter().map(|b| b.chars().count()).collect();
    assert!(
        lens.iter().max().unwrap() - lens.iter().min().unwrap() < 120,
        "{lens:?}"
    );
    assert_eq!(batches.join(" "), text);
}

#[test]
fn split_hard_slices_unbroken_runs() {
    let text = "x".repeat(700);
    let batches = split_phonemes(&text, 300);
    assert_eq!(
        batches.iter().map(|b| b.len()).collect::<Vec<_>>(),
        [300, 300, 100]
    );
    assert!(split_phonemes("   ", 300).is_empty());
}

#[test]
fn editing_one_word_changes_only_its_paragraph() {
    let b = book(&[
        "Alpha one here.",
        "Bravo two there.",
        "Charlie three everywhere.",
    ]);
    let mut settings = Settings::default();
    settings.pronunciations.clear();
    let mut project = Project::new("id", "b.epub".into(), &b, &settings);
    project.intro = false;

    let fake = Fake::default();
    let before = build_plan(&b, &project, &settings, &fake, &|_, _| {}, &NEVER).unwrap();
    project
        .pronunciations
        .push(PronEntry::new("two", SayAs::Text("too".into()), false));
    let after = build_plan(&b, &project, &settings, &fake, &|_, _| {}, &NEVER).unwrap();

    assert_ne!(before.inputs_hash, after.inputs_hash);
    let (k0, k1) = (speech_keys(&before), speech_keys(&after));
    assert_eq!(k0.len(), 3);
    assert_eq!(k0[0], k1[0]);
    assert_ne!(k0[1], k1[1]);
    assert_eq!(k0[2], k1[2]);
}

#[test]
fn selection_does_not_change_the_plan_hash_and_intro_follows_first_included() {
    let b = book(&["Hello there."]);
    let settings = Settings::default();
    let mut project = Project::new("id", "b.epub".into(), &b, &settings);
    let h = compute_inputs_hash(&project, &settings);
    project.chapters[0].included = false;
    assert_eq!(h, compute_inputs_hash(&project, &settings));

    let plan = build_plan(
        &b,
        &project,
        &settings,
        &Fake::default(),
        &|_, _| {},
        &NEVER,
    )
    .unwrap();
    assert!(!plan.intro.is_empty());
    let included: HashSet<String> = ["c1".to_string()].into();
    let seq = plan.sequence(&included);
    assert_eq!(
        seq[0].segments.len(),
        plan.intro.len() + plan.chapters[0].segments.len()
    );
    assert!(plan.sequence(&HashSet::new()).is_empty());
}

#[test]
fn tricky_words_find_unknown_words_and_acronyms() {
    let b = book(&[
        "Zorbo came to the NASA lab, and THE end came. Later Zorbo left.",
        "They used GIS and met Xii in chapter II.",
    ]);
    let settings = Settings::default();
    let mut project = Project::new("id", "b.epub".into(), &b, &settings);
    project.intro = false;
    let plan = build_plan(
        &b,
        &project,
        &settings,
        &Fake::default(),
        &|_, _| {},
        &NEVER,
    )
    .unwrap();
    let words = tricky_words(&b, &plan, &[]);
    let names: Vec<&str> = words.iter().map(|w| w.word.as_str()).collect();
    assert_eq!(words[0].word, "Zorbo");
    assert_eq!(words[0].count, 2);
    assert!(words[0].sample.starts_with("Zorbo came"));
    assert!(names.contains(&"NASA") && names.contains(&"GIS"));
    assert!(!names.contains(&"THE") && !names.contains(&"II"));
    let dismissed = tricky_words(&b, &plan, &["zorbo".into(), "nasa".into()]);
    assert!(
        !dismissed
            .iter()
            .any(|w| w.word == "Zorbo" || w.word == "NASA")
    );
}
