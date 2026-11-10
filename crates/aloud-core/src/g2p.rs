//! Text → Kokoro phonemes via misaki, with espeak-ng as the fallback for unknown words.

use misaki_rs::{
    G2P, Language,
    fallback::{EspeakFallback, Fallback, FallbackError},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Us,
    Gb,
}

impl Dialect {
    /// Voices prefixed `b` are British; everything else is American.
    pub fn for_voice(voice: &str) -> Self {
        if voice.starts_with('b') {
            Dialect::Gb
        } else {
            Dialect::Us
        }
    }
}

pub trait Phonemize: Send + Sync {
    /// Phoneme string for `text`, with the unknown-word marker removed and whitespace collapsed.
    fn phonemize(&self, dialect: Dialect, text: &str) -> Result<String>;
    /// Words that needed the espeak fallback since the last call, with counts.
    fn take_oov(&self) -> HashMap<String, u32>;
}

type OovMap = Arc<Mutex<HashMap<String, u32>>>;

struct RecordingFallback {
    inner: EspeakFallback,
    oov: OovMap,
}

impl Fallback for RecordingFallback {
    fn phonemize(&self, word: &str) -> std::result::Result<String, FallbackError> {
        if let Ok(mut m) = self.oov.lock() {
            *m.entry(word.to_lowercase()).or_insert(0) += 1;
        }
        self.inner.phonemize(word)
    }
}

pub struct Phonemizer {
    us: OnceLock<G2P>,
    gb: OnceLock<G2P>,
    oov: OovMap,
}

impl Default for Phonemizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Phonemizer {
    pub fn new() -> Self {
        Phonemizer {
            us: OnceLock::new(),
            gb: OnceLock::new(),
            oov: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn g2p(&self, dialect: Dialect) -> Result<&G2P> {
        let (cell, lang, british) = match dialect {
            Dialect::Us => (&self.us, Language::EnglishUS, false),
            Dialect::Gb => (&self.gb, Language::EnglishGB, true),
        };
        if let Some(g) = cell.get() {
            return Ok(g);
        }
        let inner = EspeakFallback::new(british).map_err(Error::Other)?;
        let fb = RecordingFallback {
            inner,
            oov: self.oov.clone(),
        };
        Ok(cell.get_or_init(|| G2P::with_fallback(lang, Some(Box::new(fb)))))
    }
}

impl Phonemize for Phonemizer {
    fn phonemize(&self, dialect: Dialect, text: &str) -> Result<String> {
        let g = self.g2p(dialect)?;
        let (ph, _tokens) = g
            .g2p(text)
            .map_err(|e| Error::Other(format!("Couldn't work out how to say some text: {e}")))?;
        Ok(ph
            .replace('❓', "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "))
    }

    fn take_oov(&self) -> HashMap<String, u32> {
        self.oov
            .lock()
            .map(|mut m| std::mem::take(&mut *m))
            .unwrap_or_default()
    }
}
