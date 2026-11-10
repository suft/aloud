use super::*;
use crate::{Error, book::ChapterKind, project::ProjectStore};
use std::io::Write;
use zip::{ZipWriter, write::SimpleFileOptions};

fn xhtml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>x</title></head><body>{body}</body></html>"#
    )
}

fn cover_png() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(20, 30, image::Rgb([200, 30, 60]));
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn build_epub(extra: &[(&str, &str)]) -> Vec<u8> {
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
 <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:title>Test Book</dc:title><dc:creator>Jane Doe</dc:creator><dc:language>en</dc:language>
  <dc:description>&lt;p&gt;A &lt;b&gt;fine&lt;/b&gt; book.&lt;/p&gt;</dc:description>
 </metadata>
 <manifest>
  <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
  <item id="cov" href="images/cover%20art.png" media-type="image/png" properties="cover-image"/>
  <item id="c0" href="cover.xhtml" media-type="application/xhtml+xml"/>
  <item id="c1" href="copyright.xhtml" media-type="application/xhtml+xml"/>
  <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  <item id="ch1b" href="ch1_split_1.xhtml" media-type="application/xhtml+xml"/>
  <item id="ch2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
 </manifest>
 <spine><itemref idref="c0"/><itemref idref="c1"/><itemref idref="ch1"/><itemref idref="ch1b"/><itemref idref="ch2"/></spine>
</package>"#;
    let nav = xhtml(
        r#"<nav epub:type="toc"><ol>
        <li><a href="copyright.xhtml">Copyright</a></li>
        <li><a href="ch1.xhtml">Chapter One</a></li>
        <li><a href="ch2.xhtml#top">Chapter Two</a></li></ol></nav>"#,
    );
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("mimetype", b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml",
            br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_vec(),
        ),
        ("OEBPS/content.opf", opf.as_bytes().to_vec()),
        ("OEBPS/nav.xhtml", nav.into_bytes()),
        ("OEBPS/images/cover art.png", cover_png()),
        ("OEBPS/cover.xhtml", xhtml(r#"<img src="images/cover%20art.png"/>"#).into_bytes()),
        (
            "OEBPS/copyright.xhtml",
            xhtml("<p>Copyright 2020 Jane Doe. All rights reserved.</p>").into_bytes(),
        ),
        (
            "OEBPS/ch1.xhtml",
            xhtml(
                r##"<h1>CHAPTER ONE</h1>
                <p><span>O</span>nce upon a time there was a book.<a epub:type="noteref" href="#n1">1</a></p>
                <p><span class="smallcaps">ALICE WAS</span> beginning to get very tired.</p>
                <aside epub:type="footnote" id="n1"><p>Footnote text</p></aside>"##,
            )
            .into_bytes(),
        ),
        (
            "OEBPS/ch1_split_1.xhtml",
            xhtml("<p>Merged text continues here.</p>").into_bytes(),
        ),
        (
            "OEBPS/ch2.xhtml",
            xhtml("<h1>Two</h1><p>First.</p><p>* * *</p><p>Second.</p>").into_bytes(),
        ),
    ];
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default();
    for (name, bytes) in files
        .iter()
        .map(|(n, b)| (*n, b.as_slice()))
        .chain(extra.iter().map(|(n, b)| (*n, b.as_bytes())))
    {
        zip.start_file(name, opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn import_bytes(bytes: &[u8]) -> (tempfile::TempDir, DataDir, Result<ProjectSummary>) {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    fs::write(&epub, bytes).unwrap();
    let data = DataDir::new(dir.path().join("data"));
    let r = import(&data, &epub);
    (dir, data, r)
}

#[test]
fn imports_chapters_titles_cover_and_cleans_text() {
    let (_d, data, r) = import_bytes(&build_epub(&[]));
    let summary = r.unwrap();
    assert_eq!(summary.title, "Test Book");
    assert!(summary.cover_path.is_some());
    assert!(summary.accent.is_some());

    let store = ProjectStore::new(data);
    let book = store.book(&summary.id).unwrap();
    let titles: Vec<_> = book.chapters.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Copyright", "Chapter One", "Chapter Two"]);
    assert_eq!(book.chapters[0].kind, ChapterKind::FrontMatter);
    assert_eq!(book.chapters[1].kind, ChapterKind::Body);
    assert_eq!(book.meta.description.as_deref(), Some("A fine book."));

    let project = store.load(&summary.id).unwrap();
    assert_eq!(
        project
            .chapters
            .iter()
            .map(|c| c.included)
            .collect::<Vec<_>>(),
        [false, true, true]
    );

    let ch1 = &book.chapters[1];
    let paras: Vec<&str> = ch1
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Para(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(paras[0], "Once upon a time there was a book.");
    assert_eq!(paras[1], "Alice was beginning to get very tired.");
    assert_eq!(paras[2], "Merged text continues here.");
    assert!(
        !ch1.blocks
            .iter()
            .any(|b| matches!(b, Block::Para(t) if t.contains("Footnote")))
    );
    // Heading is retitled, and the untitled split document is joined after a break.
    assert_eq!(ch1.blocks[0], Block::Heading("Chapter One".into()));
    assert_eq!(ch1.blocks.iter().filter(|b| **b == Block::Break).count(), 1);

    let ch2 = &book.chapters[2];
    assert_eq!(ch2.blocks.iter().filter(|b| **b == Block::Break).count(), 1);
}

#[test]
fn reimport_returns_existing_project() {
    let bytes = build_epub(&[]);
    let (dir, data, r) = import_bytes(&bytes);
    let first = r.unwrap();
    let store = ProjectStore::new(data.clone());
    let mut p = store.load(&first.id).unwrap();
    p.voice = "bf_emma".into();
    store.save(&mut p).unwrap();
    let again = import(&data, &dir.path().join("book.epub")).unwrap();
    assert_eq!(again.id, first.id);
    assert_eq!(store.load(&first.id).unwrap().voice, "bf_emma");
}

#[test]
fn drm_and_non_epub_are_rejected() {
    let (_d, _data, r) = import_bytes(&build_epub(&[("META-INF/rights.xml", "<rights/>")]));
    assert!(matches!(r, Err(Error::Import(ImportError::Drm))));

    let enc = r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/></enc:EncryptedData></encryption>"#;
    let (_d, _data, r) = import_bytes(&build_epub(&[("META-INF/encryption.xml", enc)]));
    assert!(matches!(r, Err(Error::Import(ImportError::Drm))));

    let font = r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/></enc:EncryptedData></encryption>"#;
    let (_d, _data, r) = import_bytes(&build_epub(&[("META-INF/encryption.xml", font)]));
    assert!(r.is_ok());

    let (_d, _data, r) = import_bytes(b"not a zip");
    assert!(matches!(r, Err(Error::Import(ImportError::NotEpub))));
}

#[test]
fn only_epub_files_are_imported() {
    let dir = tempfile::tempdir().unwrap();
    let data = DataDir::new(dir.path().join("data"));
    let txt = dir.path().join("book.txt");
    fs::write(&txt, build_epub(&[])).unwrap();
    assert!(matches!(
        import(&data, &txt),
        Err(Error::Import(ImportError::NotEpub))
    ));
    // A directory with an .epub name is not a file either.
    let folder = dir.path().join("folder.epub");
    fs::create_dir(&folder).unwrap();
    assert!(matches!(
        import(&data, &folder),
        Err(Error::Import(ImportError::NotEpub))
    ));
}

#[test]
fn covers_must_be_small_images() {
    let dir = tempfile::tempdir().unwrap();
    let data = DataDir::new(dir.path().join("data"));
    let settings = Settings::default();
    let book = Book {
        meta: BookMeta::default(),
        chapters: vec![],
    };
    let mut project = Project::new("0000000000000001", "b.epub".into(), &book, &settings);
    let bmp = dir.path().join("cover.bmp");
    fs::write(&bmp, cover_png()).unwrap();
    assert!(matches!(
        set_cover(&data, &mut project, &bmp),
        Err(Error::Invalid(_))
    ));
    let big = dir.path().join("big.png");
    fs::File::create(&big)
        .unwrap()
        .set_len(COVER_MAX_BYTES + 1)
        .unwrap();
    assert!(matches!(
        set_cover(&data, &mut project, &big),
        Err(Error::Invalid(_))
    ));
    fs::create_dir_all(data.project(&project.id)).unwrap();
    let ok = dir.path().join("cover.PNG");
    fs::write(&ok, cover_png()).unwrap();
    set_cover(&data, &mut project, &ok).unwrap_or_else(|e| panic!("{e}"));
}
