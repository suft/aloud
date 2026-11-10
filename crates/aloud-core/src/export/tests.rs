use super::*;
use crate::{
    book::{Block, Book, BookMeta, Chapter as BookChapter, ChapterKind},
    plan::PlannedChapter,
};

const SR: usize = SAMPLE_RATE as usize;

fn tone(secs: f32) -> Vec<f32> {
    (0..(secs * SR as f32) as usize)
        .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / SR as f32).sin() * 0.3)
        .collect()
}
fn speech(key: &str) -> Segment {
    Segment::Speech {
        key: key.repeat(16),
        phonemes: String::new(),
        speed: 1.0,
        units: 10,
    }
}

#[test]
fn exports_chapters_tags_and_duration() {
    let dir = tempfile::tempdir().unwrap();
    let data = DataDir::new(dir.path().join("data"));
    let book = Book {
        meta: BookMeta {
            title: "Tone: A Book".into(),
            authors: vec!["Jane Doe".into()],
            date: Some("2020-05-01".into()),
            cover_file: Some("cover.png".into()),
            ..Default::default()
        },
        chapters: ["c1", "c2"]
            .iter()
            .map(|id| BookChapter {
                id: id.to_string(),
                title: format!("Chapter {id}"),
                kind: ChapterKind::Body,
                blocks: vec![Block::Para("x".into())],
                words: 1,
            })
            .collect(),
    };
    let settings = Settings {
        output_dir: dir.path().join("out"),
        ..Settings::default()
    };
    let project = Project::new("proj", "t.epub".into(), &book, &settings);

    let audio = data.project_audio("proj");
    let img = image::RgbImage::from_pixel(8, 8, image::Rgb([10, 200, 30]));
    fs::create_dir_all(data.project("proj")).unwrap();
    img.save(data.project("proj").join("cover.png")).unwrap();

    // (key, seconds): c1 = 5 + 1 + 5 + 1 = 12 s, c2 = 4 + 1 + 2 + 1 = 8 s
    let clips = [("aa", 5.0f32), ("bb", 5.0), ("cc", 4.0), ("dd", 2.0)];
    for (k, s) in clips {
        cache::write_pcm(&audio, &k.repeat(16), &tone(s)).unwrap();
    }
    let sp = speech;
    let plan = Plan {
        inputs_hash: "h".into(),
        intro: vec![],
        oov: vec![],
        chapters: vec![
            PlannedChapter {
                chapter_id: "c1".into(),
                title: "Chapter c1".into(),
                segments: vec![
                    sp("aa"),
                    Segment::Silence { ms: 1000 },
                    sp("bb"),
                    Segment::Silence { ms: 1000 },
                ],
                units: 20,
            },
            PlannedChapter {
                chapter_id: "c2".into(),
                title: "Chapter c2".into(),
                segments: vec![
                    sp("cc"),
                    Segment::Silence { ms: 1000 },
                    sp("dd"),
                    Segment::Silence { ms: 1000 },
                ],
                units: 20,
            },
        ],
    };

    let info = export(&data, &project, &plan, &settings, &|_| {}).unwrap();
    assert_eq!(
        info.output.file_name().unwrap(),
        "Tone_ A Book - Jane Doe.m4b"
    );
    assert!((info.duration_secs as i64 - 20).abs() <= 1);

    let tag = Tag::read_from_path(&info.output).unwrap();
    assert_eq!(tag.chapter_list().len(), 2);
    assert_eq!(tag.chapter_track().len(), 2);
    let ch2 = tag.chapter_track()[1].start.as_secs_f64();
    assert!((ch2 - 12.0).abs() < 0.1, "chapter 2 starts at {ch2}");
    assert_eq!(tag.media_type(), Some(MediaType::AudioBook));
    assert!(tag.artwork().is_some());
    assert_eq!(tag.title(), Some("Tone: A Book"));
    assert_eq!(tag.artist(), Some("Jane Doe"));
    let dur = tag.duration().as_secs_f64();
    assert!((dur - 20.0).abs() < 0.2, "duration {dur}");

    // Missing audio is reported rather than producing a short file.
    fs::remove_file(cache::pcm_path(&audio, &"bb".repeat(16))).unwrap();
    assert!(export(&data, &project, &plan, &settings, &|_| {}).is_err());
    assert!(!info.output.with_extension("m4b.part").exists());
}

#[test]
fn long_titles_and_more_than_255_chapters_export() {
    let dir = tempfile::tempdir().unwrap();
    let data = DataDir::new(dir.path().join("data"));
    // 301 bytes, with byte 255 inside a multibyte character.
    let long_title = format!("a{}", "語".repeat(100));
    let title_of = |i: usize| {
        if i == 0 {
            long_title.clone()
        } else {
            format!("Chapter {i}")
        }
    };
    let book = Book {
        meta: BookMeta::default(),
        chapters: (0..300)
            .map(|i| BookChapter {
                id: format!("c{i}"),
                title: title_of(i),
                kind: ChapterKind::Body,
                blocks: vec![Block::Para("x".into())],
                words: 1,
            })
            .collect(),
    };
    let settings = Settings {
        output_dir: dir.path().join("out"),
        ..Settings::default()
    };
    let project = Project::new("proj", "t.epub".into(), &book, &settings);

    let audio = data.project_audio("proj");
    cache::write_pcm(&audio, &"aa".repeat(16), &tone(0.2)).unwrap();
    let plan = Plan {
        inputs_hash: "h".into(),
        intro: vec![],
        oov: vec![],
        chapters: (0..300)
            .map(|i| PlannedChapter {
                chapter_id: format!("c{i}"),
                title: title_of(i),
                segments: vec![speech("aa"), Segment::Silence { ms: 200 }],
                units: 10,
            })
            .collect(),
    };

    let info = export(&data, &project, &plan, &settings, &|_| {}).unwrap();
    let tag = Tag::read_from_path(&info.output).unwrap();
    assert_eq!(tag.chapter_track().len(), 300);
    assert_eq!(tag.chapter_list().len(), 255);
    let expected = format!("a{}", "語".repeat(84));
    assert_eq!(tag.chapter_track()[0].title, expected);
    assert_eq!(tag.chapter_list()[0].title, expected);
}

#[test]
fn output_dir_must_be_absolute_and_writable() {
    let dir = tempfile::tempdir().unwrap();
    let t = dir.path();
    assert!(matches!(
        check_output_dir(Path::new("relative/out")),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        check_output_dir(Path::new("")),
        Err(Error::Invalid(_))
    ));

    fs::write(t.join("file.txt"), b"x").unwrap();
    assert!(check_output_dir(&t.join("file.txt/sub")).is_err());

    let nested = t.join("new/nested");
    check_output_dir(&nested).unwrap();
    assert_eq!(fs::read_dir(&nested).unwrap().count(), 0);
}
