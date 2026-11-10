//! XHTML → blocks. Walks a spine document and produces headings, paragraphs and breaks.

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::sync::LazyLock;

use crate::book::Block;

static BREAK_CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)break|separator|ornament|transition|scene|space").unwrap());
static BREAK_TEXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\s*•·#~⁂❧–—-]{1,12}$").unwrap());
static NOTE_MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\d*†‡§¶,\s]+$").unwrap());
static SMALLCAPS_CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(sc|smallcaps?|small-caps?)$").unwrap());
static SMALLCAPS_STYLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)font-variant:\s*small-caps").unwrap());
static NOTE_TYPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)footnote|endnote|rearnote|pagebreak").unwrap());
static NOTE_CLASS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)footnote|pagenum").unwrap());
static UPPER_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{Lu}{2,}").unwrap());

const SKIP_TAGS: &[&str] = &[
    "script", "style", "head", "nav", "aside", "img", "svg", "noscript", "template", "title",
];
const BLOCKISH: &[&str] = &[
    "p",
    "div",
    "section",
    "article",
    "blockquote",
    "main",
    "header",
    "footer",
    "ol",
    "ul",
    "li",
    "dl",
    "dt",
    "dd",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "td",
    "th",
    "pre",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "figure",
    "figcaption",
    "hr",
    "aside",
    "body",
];

/// What the walk learned about one document.
#[derive(Debug, Default)]
pub struct Doc {
    pub blocks: Vec<Block>,
    /// `epub:type` tokens of `<body>` and its top-level sections.
    pub epub_types: Vec<String>,
    /// `src` of the first image in the document (unresolved).
    pub first_image: Option<String>,
}

pub fn extract(html: &str, book_title: &str) -> Doc {
    let dom = Html::parse_document(html);
    let mut doc = Doc::default();
    let body_sel = Selector::parse("body").unwrap();
    let Some(body) = dom.select(&body_sel).next() else {
        return doc;
    };

    doc.epub_types.extend(epub_types(body));
    for child in body.children().filter_map(ElementRef::wrap) {
        if child.value().name() == "section" {
            doc.epub_types.extend(epub_types(child));
        }
    }

    let img_sel = Selector::parse("img[src], image").unwrap();
    doc.first_image = dom.select(&img_sel).next().and_then(|e| {
        e.value()
            .attr("src")
            .or_else(|| e.value().attr("xlink:href"))
            .or_else(|| e.value().attr("href"))
            .map(str::to_string)
    });

    let mut raw = Vec::new();
    handle(body, &mut raw);
    doc.blocks = finish_blocks(raw, book_title);
    doc
}

fn epub_types(el: ElementRef) -> Vec<String> {
    el.value()
        .attr("epub:type")
        .map(|v| v.split_whitespace().map(str::to_lowercase).collect())
        .unwrap_or_default()
}

fn class_of<'a>(el: &ElementRef<'a>) -> &'a str {
    el.value().attr("class").unwrap_or("")
}

fn skip_element(el: ElementRef) -> bool {
    let v = el.value();
    let name = v.name();
    if SKIP_TAGS.contains(&name) {
        return true;
    }
    if let Some(t) = v.attr("epub:type")
        && (NOTE_TYPE.is_match(t) || (name == "a" && t.to_lowercase().contains("noteref")))
    {
        return true;
    }
    if let Some(r) = v.attr("role")
        && (r.contains("doc-noteref") || r.contains("doc-pagebreak") || r.contains("doc-footnote"))
    {
        return true;
    }
    if NOTE_CLASS.is_match(class_of(&el)) {
        return true;
    }
    if name == "sup" {
        let t: String = el.text().collect();
        if !t.trim().is_empty() && NOTE_MARK.is_match(&t) {
            return true;
        }
    }
    false
}

fn is_small_caps(el: ElementRef) -> bool {
    let v = el.value();
    v.attr("class")
        .is_some_and(|c| c.split_whitespace().any(|t| SMALLCAPS_CLASS.is_match(t)))
        || v.attr("style").is_some_and(|s| SMALLCAPS_STYLE.is_match(s))
}

fn has_block_descendant(el: ElementRef) -> bool {
    el.descendants()
        .skip(1)
        .filter_map(ElementRef::wrap)
        .any(|e| BLOCKISH.contains(&e.value().name()) && !skip_element(e))
}

fn has_direct_text(el: ElementRef) -> bool {
    el.children()
        .filter_map(|c| c.value().as_text())
        .any(|t| !t.trim().is_empty())
}

/// Text of an element with no separators at inline tag boundaries.
fn text_of(el: ElementRef) -> String {
    let mut out = String::new();
    collect_text(el, is_small_caps(el), &mut out);
    out
}

fn collect_text(el: ElementRef, small_caps: bool, out: &mut String) {
    for child in el.children() {
        if let Some(t) = child.value().as_text() {
            if small_caps {
                out.push_str(&UPPER_RUN.replace_all(t, |c: &regex::Captures| c[0].to_lowercase()));
            } else {
                out.push_str(t);
            }
        } else if let Some(e) = ElementRef::wrap(child) {
            if skip_element(e) {
                continue;
            }
            let name = e.value().name();
            if name == "br" {
                out.push(' ');
                continue;
            }
            let block = BLOCKISH.contains(&name);
            if block {
                out.push(' ');
            }
            collect_text(e, small_caps || is_small_caps(e), out);
            if block {
                out.push(' ');
            }
        }
    }
}

/// Raw walk output: blocks before cleaning rules that need whole-chapter context.
fn handle(el: ElementRef, out: &mut Vec<Block>) {
    if skip_element(el) {
        return;
    }
    let name = el.value().name();
    match name {
        "hr" => {
            out.push(Block::Break);
            return;
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let t = clean(&text_of(el));
            if t.chars().any(char::is_alphanumeric) {
                out.push(Block::Heading(t));
            }
            return;
        }
        "tr" => {
            let cells: Vec<String> = el
                .children()
                .filter_map(ElementRef::wrap)
                .filter(|c| matches!(c.value().name(), "td" | "th") && !skip_element(*c))
                .map(|c| clean(&text_of(c)))
                .filter(|t| !t.is_empty())
                .collect();
            if !cells.is_empty() {
                emit_para(&cells.join(", "), out);
            }
            return;
        }
        _ => {}
    }

    let text = || text_of(el);
    if !el.text().any(|t| t.chars().any(char::is_alphabetic)) && BREAK_CLASS.is_match(class_of(&el))
    {
        out.push(Block::Break);
        return;
    }
    // Symbol-only text (e.g. "* * *") is classified by emit_para.

    if !has_block_descendant(el) || has_direct_text(el) {
        emit_para(&text(), out);
    } else {
        for child in el.children().filter_map(ElementRef::wrap) {
            handle(child, out);
        }
    }
}

fn emit_para(raw: &str, out: &mut Vec<Block>) {
    if raw.trim().is_empty() {
        return;
    }
    if BREAK_TEXT.is_match(raw) {
        out.push(Block::Break);
        return;
    }
    let t = capitalize_first(&clean(raw));
    if t.chars().any(char::is_alphanumeric) {
        out.push(Block::Para(t));
    }
}

/// Normalizes whitespace, quotes and invisible characters; neutralizes `[` `]`.
pub fn clean(s: &str) -> String {
    let mut buf = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{2018}' | '\u{2019}' => buf.push('\''),
            '\u{201C}' | '\u{201D}' => buf.push('"'),
            '\u{00A0}' => buf.push(' '),
            '\u{00AD}' | '\u{200B}'..='\u{200D}' | '\u{FEFF}' => {}
            '[' => buf.push('('),
            ']' => buf.push(')'),
            c => buf.push(c),
        }
    }
    buf.split_whitespace()
        .map(|w| if w == "i" { "I" } else { w })
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize_first(s: &str) -> String {
    let mut done = false;
    s.chars()
        .map(|c| {
            if !done && c.is_alphanumeric() {
                done = true;
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c
            }
        })
        .collect::<String>()
}

fn is_all_upper(s: &str) -> bool {
    s.chars().any(char::is_alphabetic) && !s.chars().any(char::is_lowercase)
}

pub fn title_case(s: &str) -> String {
    let mut start = true;
    s.chars()
        .map(|c| {
            let out: String = if start && c.is_alphabetic() {
                c.to_uppercase().collect()
            } else {
                c.to_lowercase().collect()
            };
            start = c.is_whitespace() || c == '-';
            out
        })
        .collect()
}

/// Collapses breaks, trims them at the edges, retitles shouted headings and drops the book-title heading.
fn finish_blocks(raw: Vec<Block>, book_title: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::with_capacity(raw.len());
    for b in raw {
        match b {
            Block::Break => {
                if !matches!(out.last(), None | Some(Block::Break)) {
                    out.push(Block::Break);
                }
            }
            Block::Heading(h) => {
                let h = if is_all_upper(&h) { title_case(&h) } else { h };
                if h.trim().eq_ignore_ascii_case(book_title.trim())
                    || h.to_lowercase() == book_title.trim().to_lowercase()
                {
                    continue;
                }
                out.push(Block::Heading(h));
            }
            p => out.push(p),
        }
    }
    while matches!(out.last(), Some(Block::Break)) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paras(blocks: &[Block]) -> Vec<&str> {
        blocks
            .iter()
            .filter_map(|b| match b {
                Block::Para(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn drop_caps_small_caps_notes_and_breaks() {
        let html = r##"<html><body><section epub:type="chapter">
          <h2>ONE</h2>
          <p><span class="dropcap">O</span>nce upon a time.<sup>12</sup><a epub:type="noteref" href="#n1">1</a></p>
          <p><span class="smallcaps">ALICE WAS</span> beginning to get tired.</p>
          <p>* * *</p><p>&#160;</p>
          <p class="x">After &#8220;quote&#8221; and [bracket] and i am.</p>
          <aside epub:type="footnote"><p>Hidden note</p></aside>
          </section></body></html>"##;
        let d = extract(html, "Some Book");
        assert_eq!(d.blocks[0], Block::Heading("One".into()));
        assert_eq!(
            paras(&d.blocks),
            vec![
                "Once upon a time.",
                "Alice was beginning to get tired.",
                "After \"quote\" and (bracket) and I am."
            ]
        );
        assert_eq!(d.blocks.iter().filter(|b| **b == Block::Break).count(), 1);
        assert!(d.epub_types.contains(&"chapter".to_string()));
    }

    #[test]
    fn table_rows_join_cells() {
        let d = extract(
            "<body><table><tr><td>a</td><td>b</td></tr></table></body>",
            "T",
        );
        assert_eq!(paras(&d.blocks), vec!["A, b"]);
    }
}
