//! Text → Kokoro phonemes via misaki, with espeak-ng as the fallback for unknown words.

use misaki_rs::{
    G2P, Language,
    fallback::{EspeakFallback, Fallback, FallbackError},
};
use std::{
    cell::Cell,
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, PoisonError},
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
    /// Like `phonemize`, but espeak-fallback words are not recorded for `take_oov`.
    fn phonemize_untracked(&self, dialect: Dialect, text: &str) -> Result<String> {
        self.phonemize(dialect, text)
    }
}

type OovMap = Arc<Mutex<HashMap<String, u32>>>;

/// espeak-ng keeps the selected voice in global state, so every call takes this lock.
static ESPEAK: Mutex<()> = Mutex::new(());

thread_local! {
    /// False while `phonemize_untracked` runs on this thread.
    static TRACK_OOV: Cell<bool> = const { Cell::new(true) };
}

struct RecordingFallback {
    inner: EspeakFallback,
    oov: OovMap,
}

impl Fallback for RecordingFallback {
    fn phonemize(&self, word: &str) -> std::result::Result<String, FallbackError> {
        if TRACK_OOV.get()
            && let Ok(mut m) = self.oov.lock()
        {
            *m.entry(word.to_lowercase()).or_insert(0) += 1;
        }
        let _g = ESPEAK.lock().unwrap_or_else(PoisonError::into_inner);
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
        let inner = {
            let _g = ESPEAK.lock().unwrap_or_else(PoisonError::into_inner);
            EspeakFallback::new(british).map_err(Error::Other)?
        };
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

    fn phonemize_untracked(&self, dialect: Dialect, text: &str) -> Result<String> {
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                TRACK_OOV.set(true);
            }
        }
        TRACK_OOV.set(false);
        let _restore = Restore;
        self.phonemize(dialect, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the espeak-ng data: `PIPER_ESPEAKNG_DATA_DIRECTORY` (`mise run test` sets it).
    #[test]
    fn untracked_calls_skip_oov_and_threads_agree() {
        let p = Arc::new(Phonemizer::new());
        let text = "Zyxquor flimbated the brillig toves";
        let us = p.phonemize(Dialect::Us, text).unwrap();
        let gb = p.phonemize(Dialect::Gb, text).unwrap();
        assert!(p.take_oov().contains_key("zyxquor"));

        p.phonemize_untracked(Dialect::Us, "Blorptastic").unwrap();
        assert!(p.take_oov().is_empty());

        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (p, us, gb) = (p.clone(), us.clone(), gb.clone());
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        assert_eq!(p.phonemize(Dialect::Us, text).unwrap(), us);
                        assert_eq!(p.phonemize(Dialect::Gb, text).unwrap(), gb);
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
    }
}
