//! EPUB import: container/OPF parsing, TOC titles, chapter assembly, cover.

mod cover;
pub mod html;

use regex::Regex;
use roxmltree::{Document, Node};
use scraper::{Html, Selector};
use std::{
    collections::HashMap,
    fs, io,
    io::{Cursor, Read},
    path::Path,
    sync::LazyLock,
};
use zip::ZipArchive;

use crate::{
    book::{Block, Book, BookMeta, Chapter, ChapterKind},
    error::Result,
    paths::DataDir,
    project::{Project, ProjectStatus, ProjectStore, ProjectSummary},
    settings::Settings,
};

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("This file isn't an EPUB.")]
    NotEpub,
    #[error("This book is copy-protected (DRM), so Aloud can't read it.")]
    Drm,
    #[error("No readable text was found in this book.")]
    NoText,
    #[error("{}", crate::error::io_message(.0))]
    Io(#[from] io::Error),
}

static FRONT_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(cover|title ?page|half ?title|copyright|contents|table of contents|also by|praise for|other books by|books by)\b",
    )
    .unwrap()
});
static BACK_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(index|about the author|about the publisher|notes|endnotes|bibliography|newsletter|reading group guide|a note on the type|credits|discover more)\b",
    )
    .unwrap()
});
const FRONT_TYPES: &[&str] = &[
    "cover",
    "titlepage",
    "copyright-page",
    "toc",
    "halftitlepage",
];
const FONT_OBFUSCATION: &[&str] = &[
    "http://www.idpf.org/2008/embedding",
    "http://ns.adobe.com/pdf/enc#RC",
];

type Zip = ZipArchive<Cursor<Vec<u8>>>;

#[derive(Debug, Clone)]
struct Item {
    href: String,
    media_type: String,
    properties: String,
}

#[derive(Debug, Default)]
struct Opf {
    meta: BookMeta,
    manifest: HashMap<String, Item>,
    spine: Vec<String>,
    toc_id: Option<String>,
    cover_meta_id: Option<String>,
}

fn read_entry(zip: &mut Zip, name: &str) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    if let Ok(mut f) = zip.by_name(name) {
        f.read_to_end(&mut buf).ok()?;
        return Some(buf);
    }
    let found = zip
        .file_names()
        .find(|n| n.eq_ignore_ascii_case(name))
        .map(str::to_string)?;
    zip.by_name(&found).ok()?.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn read_text(zip: &mut Zip, name: &str) -> Option<String> {
    let bytes = read_entry(zip, name)?;
    let s = String::from_utf8_lossy(&bytes);
    Some(s.trim_start_matches('\u{feff}').to_string())
}

fn parse_xml(s: &str) -> Option<Document<'_>> {
    Document::parse_with_options(
        s,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .ok()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolves `href` against `base_dir` (zip-style, `/` separated); drops fragment and query.
fn resolve(base_dir: &str, href: &str) -> String {
    let href = href.split(['#', '?']).next().unwrap_or("");
    let href = percent_decode(href);
    let joined = if let Some(abs) = href.strip_prefix('/') {
        abs.to_string()
    } else if base_dir.is_empty() {
        href
    } else {
        format!("{base_dir}/{href}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn text_of_node(n: Node) -> String {
    n.descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .collect::<String>()
        .trim()
        .to_string()
}

fn strip_html(s: &str) -> String {
    let frag = Html::parse_fragment(s);
    frag.root_element()
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn check_drm(zip: &mut Zip) -> std::result::Result<(), ImportError> {
    if read_entry(zip, "META-INF/rights.xml").is_some() {
        return Err(ImportError::Drm);
    }
    if let Some(enc) = read_text(zip, "META-INF/encryption.xml")
        && let Some(doc) = parse_xml(&enc)
    {
        let protected = doc
            .descendants()
            .filter(|n| n.has_tag_name("EncryptionMethod"))
            .filter_map(|n| n.attribute("Algorithm"))
            .any(|a| !FONT_OBFUSCATION.contains(&a));
        if protected {
            return Err(ImportError::Drm);
        }
    }
    Ok(())
}

fn parse_opf(xml: &str, opf_dir: &str) -> Option<Opf> {
    let doc = parse_xml(xml)?;
    let root = doc.root_element();
    let mut opf = Opf::default();
    let child = |name: &str| root.children().find(|n| n.has_tag_name(name));

    if let Some(md) = child("metadata") {
        for n in md.children().filter(|n| n.is_element()) {
            let t = text_of_node(n);
            match n.tag_name().name() {
                "title" if opf.meta.title.is_empty() => opf.meta.title = t,
                "creator" if !t.is_empty() => opf.meta.authors.push(t),
                "language" if opf.meta.language.is_none() && !t.is_empty() => {
                    opf.meta.language = Some(t)
                }
                "publisher" if opf.meta.publisher.is_none() && !t.is_empty() => {
                    opf.meta.publisher = Some(t)
                }
                "date" if opf.meta.date.is_none() && !t.is_empty() => opf.meta.date = Some(t),
                "description" if opf.meta.description.is_none() && !t.is_empty() => {
                    let d = strip_html(&t);
                    if !d.is_empty() {
                        opf.meta.description = Some(d);
                    }
                }
                "meta" if n.attribute("name") == Some("cover") => {
                    opf.cover_meta_id = n.attribute("content").map(str::to_string)
                }
                _ => {}
            }
        }
    }
    if let Some(mf) = child("manifest") {
        for n in mf.children().filter(|n| n.has_tag_name("item")) {
            if let (Some(id), Some(href)) = (n.attribute("id"), n.attribute("href")) {
                opf.manifest.insert(
                    id.to_string(),
                    Item {
                        href: resolve(opf_dir, href),
                        media_type: n.attribute("media-type").unwrap_or("").to_string(),
                        properties: n.attribute("properties").unwrap_or("").to_string(),
                    },
                );
            }
        }
    }
    if let Some(sp) = child("spine") {
        opf.toc_id = sp.attribute("toc").map(str::to_string);
        for n in sp.children().filter(|n| n.has_tag_name("itemref")) {
            if n.attribute("linear") == Some("no") {
                continue;
            }
            if let Some(idref) = n.attribute("idref") {
                opf.spine.push(idref.to_string());
            }
        }
    }
    Some(opf)
}

fn add_toc(map: &mut HashMap<String, String>, path: String, label: String) {
    let label = html::clean(&label);
    if !label.is_empty() {
        map.entry(path).or_insert(label);
    }
}

/// Map from document path (no fragment) to the first TOC label that points at it.
fn toc_titles(zip: &mut Zip, opf: &Opf) -> HashMap<String, String> {
    let mut map = HashMap::new();

    let nav = opf
        .manifest
        .values()
        .find(|i| i.properties.split_whitespace().any(|p| p == "nav"))
        .cloned();
    if let Some(nav) = nav
        && let Some(text) = read_text(zip, &nav.href)
    {
        let dom = Html::parse_document(&text);
        let nav_sel = Selector::parse("nav").unwrap();
        let a_sel = Selector::parse("a[href]").unwrap();
        let navs: Vec<_> = dom.select(&nav_sel).collect();
        let chosen = navs
            .iter()
            .find(|n| {
                n.value()
                    .attr("epub:type")
                    .is_some_and(|t| t.split_whitespace().any(|w| w == "toc"))
            })
            .or_else(|| navs.first());
        if let Some(n) = chosen {
            let base = dir_of(&nav.href).to_string();
            for a in n.select(&a_sel) {
                let href = a.value().attr("href").unwrap_or("");
                add_toc(&mut map, resolve(&base, href), a.text().collect::<String>());
            }
        }
    }
    if map.is_empty() {
        let ncx = opf
            .toc_id
            .as_ref()
            .and_then(|id| opf.manifest.get(id))
            .cloned();
        if let Some(ncx) = ncx
            && let Some(text) = read_text(zip, &ncx.href)
            && let Some(doc) = parse_xml(&text)
        {
            let base = dir_of(&ncx.href).to_string();
            for np in doc.descendants().filter(|n| n.has_tag_name("navPoint")) {
                let src = np
                    .children()
                    .find(|c| c.has_tag_name("content"))
                    .and_then(|c| c.attribute("src"));
                let label = np
                    .children()
                    .find(|c| c.has_tag_name("navLabel"))
                    .map(text_of_node);
                if let (Some(src), Some(label)) = (src, label) {
                    add_toc(&mut map, resolve(&base, src), label);
                }
            }
        }
    }
    map
}

fn kind_of(title: &str, epub_types: &[String]) -> ChapterKind {
    if FRONT_TITLE.is_match(title) || epub_types.iter().any(|t| FRONT_TYPES.contains(&t.as_str())) {
        ChapterKind::FrontMatter
    } else if BACK_TITLE.is_match(title) {
        ChapterKind::BackMatter
    } else {
        ChapterKind::Body
    }
}

fn find_cover(zip: &mut Zip, opf: &Opf, first_doc: Option<(&str, &html::Doc)>) -> Option<Vec<u8>> {
    let by_prop = opf
        .manifest
        .values()
        .find(|i| i.properties.split_whitespace().any(|p| p == "cover-image"));
    let by_meta = opf
        .cover_meta_id
        .as_ref()
        .and_then(|id| opf.manifest.get(id))
        .filter(|i| i.media_type.starts_with("image/"));
    if let Some(item) = by_prop.or(by_meta) {
        return read_entry(zip, &item.href);
    }
    let (path, doc) = first_doc?;
    let text_free = !doc.blocks.iter().any(|b| matches!(b, Block::Para(_)));
    let front = doc
        .epub_types
        .iter()
        .any(|t| FRONT_TYPES.contains(&t.as_str()));
    if text_free || front {
        let src = doc.first_image.as_ref()?;
        return read_entry(zip, &resolve(dir_of(path), src));
    }
    None
}

struct Parsed {
    book: Book,
    cover: Option<Vec<u8>>,
}

fn parse_epub(bytes: Vec<u8>, fallback_title: &str) -> std::result::Result<Parsed, ImportError> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| ImportError::NotEpub)?;
    check_drm(&mut zip)?;

    let container = read_text(&mut zip, "META-INF/container.xml").ok_or(ImportError::NotEpub)?;
    let opf_path = {
        let doc = parse_xml(&container).ok_or(ImportError::NotEpub)?;
        doc.descendants()
            .find(|n| n.has_tag_name("rootfile"))
            .and_then(|n| n.attribute("full-path"))
            .map(|p| resolve("", p))
            .ok_or(ImportError::NotEpub)?
    };
    let opf_text = read_text(&mut zip, &opf_path).ok_or(ImportError::NotEpub)?;
    let opf_dir = dir_of(&opf_path).to_string();
    let mut opf = parse_opf(&opf_text, &opf_dir).ok_or(ImportError::NotEpub)?;
    if opf.meta.title.trim().is_empty() {
        opf.meta.title = fallback_title.to_string();
    }
    let title = html::clean(&opf.meta.title);
    opf.meta.title = title.clone();
    let titles = toc_titles(&mut zip, &opf);

    let mut chapters: Vec<Chapter> = Vec::new();
    let mut section_n = 0;
    let mut first_doc: Option<(String, html::Doc)> = None;
    for (idx, idref) in opf.spine.iter().enumerate() {
        let Some(item) = opf.manifest.get(idref) else {
            continue;
        };
        let Some(text) = read_text(&mut zip, &item.href) else {
            continue;
        };
        let doc = html::extract(&text, &title);
        if idx == 0 {
            // Keep only what cover detection needs; the blocks are consumed below.
            first_doc = Some((
                item.href.clone(),
                html::Doc {
                    blocks: doc.blocks.clone(),
                    epub_types: doc.epub_types.clone(),
                    first_image: doc.first_image.clone(),
                },
            ));
        }
        if !doc.blocks.iter().any(|b| matches!(b, Block::Para(_))) {
            continue;
        }
        let doc_title = titles.get(&item.href).cloned().or_else(|| {
            doc.blocks.iter().find_map(|b| match b {
                Block::Heading(h) => Some(h.clone()),
                _ => None,
            })
        });
        match doc_title {
            None => {
                if let Some(prev) = chapters.last_mut().filter(|c| c.kind == ChapterKind::Body) {
                    prev.blocks.push(Block::Break);
                    prev.blocks.extend(doc.blocks);
                    prev.words = Chapter::count_words(&prev.blocks);
                } else {
                    section_n += 1;
                    let words = Chapter::count_words(&doc.blocks);
                    chapters.push(Chapter {
                        id: idref.clone(),
                        title: format!("Section {section_n}"),
                        kind: kind_of("", &doc.epub_types),
                        blocks: doc.blocks,
                        words,
                    });
                }
            }
            Some(t) => {
                let words = Chapter::count_words(&doc.blocks);
                chapters.push(Chapter {
                    id: idref.clone(),
                    kind: kind_of(&t, &doc.epub_types),
                    title: t,
                    blocks: doc.blocks,
                    words,
                });
            }
        }
    }

    if !chapters
        .iter()
        .any(|c| c.kind == ChapterKind::Body && c.words > 0)
    {
        return Err(ImportError::NoText);
    }

    let cover = find_cover(
        &mut zip,
        &opf,
        first_doc.as_ref().map(|(p, d)| (p.as_str(), d)),
    );
    Ok(Parsed {
        book: Book {
            meta: opf.meta,
            chapters,
        },
        cover,
    })
}

/// Imports an EPUB into the data dir. Re-importing identical bytes returns the existing project.
pub fn import(data: &DataDir, epub_path: &Path) -> Result<ProjectSummary> {
    let is_epub = epub_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("epub"));
    if !is_epub || !fs::metadata(epub_path).map_err(ImportError::Io)?.is_file() {
        return Err(ImportError::NotEpub.into());
    }
    let bytes = fs::read(epub_path).map_err(ImportError::Io)?;
    let id: String = blake3::hash(&bytes).to_hex()[..16].to_string();
    let store = ProjectStore::new(data.clone());
    let settings = Settings::load(data)?;

    if store.exists(&id) {
        let p = store.load(&id)?;
        return Ok(store.summary(&p, &settings));
    }

    let stem = epub_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Parsed { mut book, cover } = parse_epub(bytes, &stem)?;

    let dir = data.project(&id);
    fs::create_dir_all(&dir)?;
    if let Some(bytes) = cover
        && let Some((name, out)) = cover::prepare(&bytes)
    {
        fs::write(dir.join(name), out)?;
        book.meta.cover_file = Some(name.to_string());
        book.meta.accent = cover::accent(&bytes);
    }
    store.save_book(&id, &book)?;
    let file_name = epub_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut project = Project::new(&id, file_name, &book, &settings);
    project.status = ProjectStatus::Draft;
    // project.json is written last: its presence marks a complete import.
    store.save(&mut project)?;
    Ok(store.summary(&project, &settings))
}

const COVER_MAX_BYTES: u64 = 50 * 1024 * 1024;

/// Copies a user-chosen image in as the project cover.
pub fn set_cover(data: &DataDir, project: &mut Project, image_path: &Path) -> Result<()> {
    let ext = image_path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(ext.as_deref(), Some("jpg" | "jpeg" | "png" | "webp")) {
        return Err(crate::Error::Invalid(
            "Choose a JPEG, PNG or WebP image.".into(),
        ));
    }
    let meta = fs::metadata(image_path)?;
    if !meta.is_file() {
        return Err(crate::Error::Invalid("Choose an image file.".into()));
    }
    if meta.len() > COVER_MAX_BYTES {
        return Err(crate::Error::Invalid(
            "That image is too large. Choose one under 50 MB.".into(),
        ));
    }
    let bytes = fs::read(image_path)?;
    let (name, out) = cover::prepare(&bytes)
        .ok_or_else(|| crate::Error::Invalid("That image couldn't be read.".into()))?;
    let dir = data.project(&project.id);
    for old in ["cover.jpg", "cover.png"] {
        let _ = fs::remove_file(dir.join(old));
    }
    fsx_write(&dir.join(name), &out)?;
    project.meta.cover_file = Some(name.to_string());
    project.meta.accent = cover::accent(&bytes);
    Ok(())
}

fn fsx_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

#[cfg(test)]
mod tests;
