//! Book → plan: the ordered list of speech and silence segments to synthesize.
//!
//! A plan depends only on the text and the settings that shape the audio, never on which
//! chapters are selected. Segment keys are content hashes, so editing one word invalidates
//! only the segments that contain it.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{
    book::{Block, Book, ChapterKind},
    error::{Error, Result},
    g2p::{Dialect, Phonemize},
    project::Project,
    pron::{self, Dictionary, SayAs},
    settings::{ModelVariant, Settings},
    speed, tts,
};

pub const PLAN_VERSION: u32 = 1;
pub const MAX_BATCH_PHONEMES: usize = 300;

const HEADING_SPEED: f32 = 0.9;
const SENTENCE_MARKS: &str = ".!?…";
const CLAUSE_MARKS: &str = ",;:";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Segment {
    Speech {
        key: String,
        phonemes: String,
        speed: f32,
        units: u32,
    },
    Silence {
        ms: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedChapter {
    pub chapter_id: String,
    pub title: String,
    pub segments: Vec<Segment>,
    pub units: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub inputs_hash: String,
    /// Spoken introduction; prepended to the first *included* chapter by [`Plan::sequence`].
    pub intro: Vec<Segment>,
    pub chapters: Vec<PlannedChapter>,
    pub oov: Vec<(String, u32)>,
}

/// One chapter of the audiobook with its segments in playback order.
pub struct SeqChapter<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub segments: Vec<&'a Segment>,
}

impl Plan {
    /// Every speech key of the intro and all chapters, included or not.
    pub fn speech_keys(&self) -> HashSet<&str> {
        self.intro
            .iter()
            .chain(self.chapters.iter().flat_map(|c| &c.segments))
            .filter_map(|s| match s {
                Segment::Speech { key, .. } => Some(key.as_str()),
                Segment::Silence { .. } => None,
            })
            .collect()
    }

    /// Included chapters in book order; the intro is placed at the start of the first of them.
    pub fn sequence(&self, included: &HashSet<String>) -> Vec<SeqChapter<'_>> {
        let mut out: Vec<SeqChapter> = self
            .chapters
            .iter()
            .filter(|c| included.contains(&c.chapter_id))
            .map(|c| SeqChapter {
                id: &c.chapter_id,
                title: &c.title,
                segments: c.segments.iter().collect(),
            })
            .collect();
        if let Some(first) = out.first_mut() {
            let mut segs: Vec<&Segment> = self.intro.iter().collect();
            segs.append(&mut first.segments);
            first.segments = segs;
        }
        out
    }
}

impl Segment {
    pub fn units(&self) -> u64 {
        match self {
            Segment::Speech { units, .. } => *units as u64,
            Segment::Silence { .. } => 0,
        }
    }
}

#[derive(Serialize)]
struct PlanInputs<'a> {
    plan_version: u32,
    model: ModelVariant,
    voice: &'a str,
    speed: f32,
    intro: bool,
    title: &'a str,
    authors: &'a [String],
    book_id: &'a str,
    chapter_titles: Vec<(&'a str, &'a str)>,
    prons: Vec<(&'a str, &'a SayAs, bool)>,
}

/// Hash of everything (except chapter selection) that shapes a plan.
pub fn compute_inputs_hash(project: &Project, settings: &Settings) -> String {
    let prons = pron::effective_entries(&settings.pronunciations, &project.pronunciations)
        .into_iter()
        .map(|e| (e.word.as_str(), &e.say_as, e.match_case))
        .collect();
    let inputs = PlanInputs {
        plan_version: PLAN_VERSION,
        model: settings.model,
        voice: &project.voice,
        speed: project.speed,
        intro: project.intro,
        title: &project.meta.title,
        authors: &project.meta.authors,
        book_id: &project.id,
        chapter_titles: project
            .chapters
            .iter()
            .map(|c| (c.id.as_str(), c.title.as_str()))
            .collect(),
        prons,
    };
    let bytes = serde_json::to_vec(&inputs).unwrap_or_default();
    blake3::hash(&bytes).to_hex().to_string()
}

/// Cache key of one speech segment: identical text with identical settings shares audio.
pub fn segment_key(model: ModelVariant, voice: &str, speed: f32, phonemes: &str) -> String {
    let s = format!(
        "{PLAN_VERSION}|{}|{voice}|{speed:.2}|{phonemes}",
        model.as_str()
    );
    blake3::hash(s.as_bytes()).to_hex()[..32].to_string()
}

pub fn only_vocab(ph: &str) -> String {
    let vocab = tts::vocab();
    ph.chars().filter(|c| vocab.contains_key(c)).collect()
}

fn speech(model: ModelVariant, voice: &str, speed: f32, phonemes: String) -> Option<Segment> {
    let phonemes = phonemes.trim().to_string();
    if phonemes.is_empty() {
        return None;
    }
    Some(Segment::Speech {
        key: segment_key(model, voice, speed, &phonemes),
        units: phonemes.chars().count() as u32,
        phonemes,
        speed,
    })
}

fn join_authors(authors: &[String]) -> String {
    match authors {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

fn with_stop(s: &str) -> String {
    if s.trim_end().ends_with(['.', '!', '?', '…', ':']) {
        s.to_string()
    } else {
        format!("{s}.")
    }
}

/// Plans one paragraph: speech batches joined by pauses, closed by a paragraph pause.
fn plan_para(
    text: &str,
    dict: &Dictionary,
    ph: &dyn Phonemize,
    dialect: Dialect,
    (model, voice, speed): (ModelVariant, &str, f32),
    out: &mut Vec<Segment>,
) -> Result<()> {
    let applied = dict.apply(text);
    let phonemes = only_vocab(&ph.phonemize(dialect, &applied)?);
    let batches = split_phonemes(&phonemes, MAX_BATCH_PHONEMES);
    let n = batches.len();
    for (i, b) in batches.into_iter().enumerate() {
        let last_char = b.trim_end().chars().last();
        if let Some(seg) = speech(model, voice, speed, b) {
            out.push(seg);
            if i + 1 < n {
                let ms = match last_char {
                    Some(c) if SENTENCE_MARKS.contains(c) => 300,
                    Some(c) if CLAUSE_MARKS.contains(c) => 150,
                    _ => 50,
                };
                out.push(Segment::Silence { ms });
            }
        }
    }
    if n > 0 {
        out.push(Segment::Silence { ms: 450 });
    }
    Ok(())
}

pub fn build_plan(
    book: &Book,
    project: &Project,
    settings: &Settings,
    ph: &dyn Phonemize,
    progress: &dyn Fn(usize, usize),
    cancel: &AtomicBool,
) -> Result<Plan> {
    let dict = Dictionary::new(&settings.pronunciations, &project.pronunciations)?;
    let dialect = Dialect::for_voice(&project.voice);
    let (model, voice, speed) = (
        settings.model,
        project.voice.as_str(),
        speed::clamp(project.speed),
    );
    let heading_speed = speed * HEADING_SPEED;
    let titles: HashMap<&str, &str> = project
        .chapters
        .iter()
        .map(|c| (c.id.as_str(), c.title.as_str()))
        .collect();

    let mut plan = Plan {
        inputs_hash: compute_inputs_hash(project, settings),
        ..Plan::default()
    };

    if project.intro {
        let mut intro = Vec::new();
        let title = format!("{}.", project.meta.title.trim_end_matches('.'));
        let say = |text: &str| -> Result<Option<Segment>> {
            let applied = dict.apply(text);
            let p = only_vocab(&ph.phonemize(dialect, &applied)?);
            Ok(speech(model, voice, heading_speed, p))
        };
        intro.extend(say(&title)?);
        intro.push(Segment::Silence { ms: 700 });
        if !project.meta.authors.is_empty() {
            intro.extend(say(&format!(
                "By {}.",
                join_authors(&project.meta.authors)
            ))?);
        }
        intro.push(Segment::Silence { ms: 1500 });
        plan.intro = intro;
    }

    let total = book.chapters.len();
    for (ci, ch) in book.chapters.iter().enumerate() {
        let mut segments = Vec::new();
        for block in &ch.blocks {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            match block {
                Block::Heading(h) => {
                    segments.push(Segment::Silence { ms: 1200 });
                    let applied = dict.apply(&with_stop(h)).into_owned();
                    let p = only_vocab(&ph.phonemize(dialect, &applied)?);
                    segments.extend(speech(model, voice, heading_speed, p));
                    segments.push(Segment::Silence { ms: 900 });
                }
                Block::Break => segments.push(Segment::Silence { ms: 1300 }),
                Block::Para(t) => {
                    plan_para(t, &dict, ph, dialect, (model, voice, speed), &mut segments)?
                }
            }
        }
        let units = segments.iter().map(Segment::units).sum();
        plan.chapters.push(PlannedChapter {
            chapter_id: ch.id.clone(),
            title: titles
                .get(ch.id.as_str())
                .unwrap_or(&ch.title.as_str())
                .to_string(),
            segments,
            units,
        });
        progress(ci + 1, total);
    }

    let mut oov: Vec<(String, u32)> = ph
        .take_oov()
        .into_iter()
        .filter(|(w, _)| {
            w.chars().filter(|c| c.is_alphabetic()).count() >= 2
                && w.chars().all(|c| c.is_alphabetic() || c == '\'')
        })
        .collect();
    oov.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    plan.oov = oov;
    Ok(plan)
}

// --- phoneme batching (port of kokoro_onnx/chunker.py) ---

fn is_boundary_mark(level: usize, c: char) -> bool {
    match level {
        0 => SENTENCE_MARKS.contains(c),
        1 => CLAUSE_MARKS.contains(c),
        _ => true,
    }
}

/// Splits at whitespace runs that follow a mark of the given level (any run for level 2).
fn split_boundary(s: &str, level: usize) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0;
    // Last non-whitespace char seen, and where the current whitespace run began.
    let mut prev: Option<char> = None;
    let mut run_start: Option<usize> = None;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            if run_start.is_none() && prev.is_some() {
                run_start = Some(i);
            }
        } else {
            if let (Some(rs), Some(p)) = (run_start.take(), prev)
                && is_boundary_mark(level, p)
            {
                pieces.push(&s[start..rs]);
                start = i;
            }
            prev = Some(c);
        }
    }
    pieces.push(&s[start..]);
    pieces
}

fn atoms(ph: &str, max: usize, level: usize, out: &mut Vec<String>) {
    if ph.chars().count() <= max {
        if !ph.is_empty() {
            out.push(ph.to_string());
        }
        return;
    }
    for index in level..3 {
        let pieces = split_boundary(ph, index);
        if pieces.len() > 1 {
            for piece in pieces {
                atoms(piece.trim(), max, index + 1, out);
            }
            return;
        }
    }
    let chars: Vec<char> = ph.chars().collect();
    out.extend(chars.chunks(max).map(|c| c.iter().collect::<String>()));
}

fn pack(lengths: &[usize], limit: usize) -> Vec<(usize, usize)> {
    let mut batches = Vec::new();
    let (mut start, mut size) = (0, 0);
    for (i, &len) in lengths.iter().enumerate() {
        let candidate = if i == start { len } else { size + 1 + len };
        if candidate > limit && i > start {
            batches.push((start, i));
            start = i;
            size = len;
        } else {
            size = candidate;
        }
    }
    if !lengths.is_empty() {
        batches.push((start, lengths.len()));
    }
    batches
}

/// Splits phonemes into balanced batches of at most `max` characters.
pub fn split_phonemes(phonemes: &str, max: usize) -> Vec<String> {
    let mut atoms_v = Vec::new();
    atoms(phonemes.trim(), max, 0, &mut atoms_v);
    if atoms_v.is_empty() {
        return Vec::new();
    }
    let lengths: Vec<usize> = atoms_v.iter().map(|a| a.chars().count()).collect();
    let fewest = pack(&lengths, max).len();
    let (mut low, mut high) = (lengths.iter().copied().max().unwrap_or(1), max);
    while low < high {
        let mid = (low + high) / 2;
        if pack(&lengths, mid).len() <= fewest {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    pack(&lengths, low)
        .into_iter()
        .map(|(s, e)| atoms_v[s..e].join(" "))
        .collect()
}

// --- tricky words ---

#[derive(ts_rs::TS, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrickyWord {
    pub word: String,
    pub count: u32,
    pub sample: String,
}

static TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{L}[\p{L}']*").unwrap());
static ACRONYM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-Z]{2,6}\b").unwrap());
static ROMAN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[IVXLC]+$").unwrap());

fn title_of(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// The sentence around `[start, end)`, cut to about ±80 characters at word boundaries.
fn sample_around(text: &str, start: usize, end: usize) -> String {
    let before = &text[..start];
    let s_start = before
        .rfind(['.', '!', '?'])
        .map(|i| {
            let rest = &before[i + 1..];
            i + 1 + (rest.len() - rest.trim_start().len())
        })
        .unwrap_or(0);
    let after = &text[end..];
    let s_end = after
        .find(['.', '!', '?'])
        .map(|i| end + i + 1)
        .unwrap_or(text.len());
    let sentence = &text[s_start..s_end];
    let n = sentence.chars().count();
    if n <= 160 {
        return sentence.trim().to_string();
    }
    let rel = start - s_start;
    let from = floor_boundary(sentence, rel.saturating_sub(80));
    let to = ceil_boundary(sentence, (rel + (end - start) + 80).min(sentence.len()));
    let mut cut = &sentence[from..to];
    if from > 0
        && let Some(i) = cut.find(' ')
    {
        cut = &cut[i + 1..];
    }
    if to < sentence.len()
        && let Some(i) = cut.rfind(' ')
    {
        cut = &cut[..i];
    }
    format!(
        "{}{}{}",
        if from > 0 { "…" } else { "" },
        cut.trim(),
        if to < sentence.len() { "…" } else { "" }
    )
}

pub(crate) fn floor_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}
fn ceil_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Words the voice is likely to get wrong: unknown words and acronyms.
pub fn tricky_words(book: &Book, plan: &Plan, dismissed: &[String]) -> Vec<TrickyWord> {
    let dismissed: HashSet<String> = dismissed.iter().map(|d| d.to_lowercase()).collect();
    let paras: Vec<&str> = book
        .chapters
        .iter()
        .filter(|c| c.kind == ChapterKind::Body)
        .flat_map(|c| c.blocks.iter())
        .filter_map(|b| match b {
            Block::Para(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();

    // Case-sensitive vocabulary of the book, to tell acronyms from shouted words.
    let mut seen: HashSet<&str> = HashSet::new();
    for p in &paras {
        seen.extend(TOKEN.find_iter(p).map(|m| m.as_str()));
    }

    // key (lowercase) → (display word, count)
    let mut found: HashMap<String, (String, u32)> = HashMap::new();
    for (w, c) in &plan.oov {
        found.insert(w.clone(), (w.clone(), *c));
    }
    let mut acr_counts: HashMap<&str, u32> = HashMap::new();
    for p in &paras {
        for m in ACRONYM.find_iter(p) {
            let w = m.as_str();
            if ROMAN.is_match(w)
                || seen.contains(w.to_lowercase().as_str())
                || seen.contains(title_of(w).as_str())
            {
                continue;
            }
            *acr_counts.entry(w).or_insert(0) += 1;
        }
    }
    for (w, c) in acr_counts {
        found.entry(w.to_lowercase()).or_insert((w.to_string(), c));
    }
    found.retain(|k, _| !dismissed.contains(k));

    // Samples and display capitalization from the first occurrence in the text.
    let mut samples: HashMap<String, (String, String)> = HashMap::new();
    for p in &paras {
        if samples.len() == found.len() {
            break;
        }
        for m in TOKEN.find_iter(p) {
            let key = m.as_str().to_lowercase();
            if found.contains_key(&key) && !samples.contains_key(&key) {
                samples.insert(
                    key,
                    (m.as_str().to_string(), sample_around(p, m.start(), m.end())),
                );
            }
        }
    }

    let mut out: Vec<TrickyWord> = found
        .into_iter()
        .map(|(key, (word, count))| {
            let (shown, sample) = samples.remove(&key).unwrap_or((word.clone(), word.clone()));
            let word = if word.chars().all(|c| c.is_uppercase()) {
                word
            } else {
                shown
            };
            TrickyWord {
                word,
                count,
                sample,
            }
        })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word)));
    out.truncate(200);
    out
}

#[cfg(test)]
mod tests;
