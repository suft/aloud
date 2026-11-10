use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BookMeta {
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub date: Option<String>,
    pub description: Option<String>,
    /// `cover.jpg` / `cover.png` inside the project directory.
    pub cover_file: Option<String>,
    /// Dominant cover color as `#rrggbb`.
    pub accent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChapterKind {
    Body,
    FrontMatter,
    BackMatter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum Block {
    Heading(String),
    Para(String),
    Break,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    /// First spine idref of the chapter.
    pub id: String,
    pub title: String,
    pub kind: ChapterKind,
    pub blocks: Vec<Block>,
    pub words: u32,
}

/// Immutable result of an import, stored as `projects/<id>/book.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Book {
    pub meta: BookMeta,
    pub chapters: Vec<Chapter>,
}

impl Chapter {
    pub fn count_words(blocks: &[Block]) -> u32 {
        blocks
            .iter()
            .map(|b| match b {
                Block::Heading(t) | Block::Para(t) => t.split_whitespace().count() as u32,
                Block::Break => 0,
            })
            .sum()
    }
}
