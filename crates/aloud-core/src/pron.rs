//! Pronunciation dictionary: user fixes applied to text before phonemization.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashMap, sync::LazyLock};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum SayAs {
    /// Replacement text, e.g. "Cha-ree-an-dee".
    Text(String),
    /// Raw Kokoro phonemes.
    Phonemes(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PronEntry {
    pub id: String,
    pub word: String,
    pub say_as: SayAs,
    #[serde(default)]
    pub match_case: bool,
}

static WORD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[\p{L}\p{N}][\p{L}\p{N}' .\-]*[\p{L}\p{N}]|[\p{L}\p{N}])$").unwrap()
});

const BAD_WORD: &str = "Use letters or numbers (spaces, hyphens and apostrophes are fine inside).";

impl PronEntry {
    pub fn new(word: &str, say_as: SayAs, match_case: bool) -> Self {
        PronEntry {
            id: uuid::Uuid::new_v4().to_string(),
            word: word.trim().to_string(),
            say_as,
            match_case,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let w = self.word.trim();
        let n = w.chars().count();
        if !(1..=64).contains(&n) || !WORD_RE.is_match(w) {
            return Err(Error::Invalid(BAD_WORD.into()));
        }
        match &self.say_as {
            SayAs::Text(t) if t.trim().is_empty() => {
                Err(Error::Invalid("Type how the word should sound.".into()))
            }
            SayAs::Phonemes(p) => {
                let vocab = crate::tts::vocab();
                if p.trim().is_empty() || !p.chars().all(|c| vocab.contains_key(&c)) {
                    Err(Error::Invalid(
                        "That phonetic spelling contains symbols Aloud doesn't know.".into(),
                    ))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
}

/// Book entries override global entries with the same (case-insensitive) word.
/// Sorted by lowercase word so the result is stable for hashing.
pub fn effective_entries<'a>(global: &'a [PronEntry], book: &'a [PronEntry]) -> Vec<&'a PronEntry> {
    let mut map: HashMap<String, &PronEntry> = HashMap::new();
    for e in global.iter().chain(book) {
        map.insert(e.word.to_lowercase(), e);
    }
    let mut v: Vec<&PronEntry> = map.into_values().collect();
    v.sort_by_key(|a| a.word.to_lowercase());
    v
}

pub struct Dictionary {
    re: Option<Regex>,
    by_word: HashMap<String, PronEntry>,
}

impl Dictionary {
    pub fn new(global: &[PronEntry], book: &[PronEntry]) -> Self {
        let entries = effective_entries(global, book);
        if entries.is_empty() {
            return Dictionary {
                re: None,
                by_word: HashMap::new(),
            };
        }
        let mut words: Vec<&str> = entries.iter().map(|e| e.word.as_str()).collect();
        words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
        let alt = words
            .iter()
            .map(|w| regex::escape(w))
            .collect::<Vec<_>>()
            .join("|");
        let re = Regex::new(&format!(r"(?i)\b(?:{alt})\b")).ok();
        let by_word = entries
            .into_iter()
            .map(|e| (e.word.to_lowercase(), e.clone()))
            .collect();
        Dictionary { re, by_word }
    }

    pub fn apply<'t>(&self, text: &'t str) -> Cow<'t, str> {
        let Some(re) = &self.re else {
            return Cow::Borrowed(text);
        };
        re.replace_all(text, |caps: &regex::Captures| {
            let m = &caps[0];
            match self.by_word.get(&m.to_lowercase()) {
                Some(e) if e.match_case && e.word != m => m.to_string(),
                Some(e) => match &e.say_as {
                    SayAs::Text(v) => v.clone(),
                    SayAs::Phonemes(p) => format!("[{m}](/{p}/)"),
                },
                None => m.to_string(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(word: &str, to: &str, match_case: bool) -> PronEntry {
        PronEntry::new(word, SayAs::Text(to.into()), match_case)
    }

    #[test]
    fn match_case_and_whole_words() {
        let d = Dictionary::new(&[], &[text("IT", "I T", true), text("Al", "Albert", false)]);
        assert_eq!(
            d.apply("it is IT and Alice met al."),
            "it is I T and Alice met Albert."
        );
    }

    #[test]
    fn longest_match_wins_and_book_overrides_global() {
        let global = [
            text("New York", "Noo Yawk", false),
            text("York", "Yorkshire", false),
        ];
        let book = [text("york", "Yark", false)];
        let d = Dictionary::new(&global, &book);
        assert_eq!(d.apply("New York and York"), "Noo Yawk and Yark");
    }

    #[test]
    fn phoneme_entries_emit_misaki_markup() {
        let e = PronEntry::new("Chariandy", SayAs::Phonemes("ʧˈɑɹiɑndi".into()), false);
        let d = Dictionary::new(&[], &[e]);
        assert_eq!(d.apply("Mr Chariandy."), "Mr [Chariandy](/ʧˈɑɹiɑndi/).");
    }

    #[test]
    fn empty_dictionary_borrows() {
        let d = Dictionary::new(&[], &[]);
        assert!(matches!(d.apply("hello"), Cow::Borrowed(_)));
    }

    #[test]
    fn validation() {
        assert!(text("-x", "y", false).validate().is_err());
        assert!(text("St. John's", "Sinjun", false).validate().is_ok());
        assert!(text("a", "b", false).validate().is_ok());
        assert!(text("ok", " ", false).validate().is_err());
        let bad = PronEntry::new("ok", SayAs::Phonemes("#$%".into()), false);
        assert!(bad.validate().is_err());
    }
}
