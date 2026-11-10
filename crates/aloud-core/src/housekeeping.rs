//! Startup clean-up of files that crashes or failed operations leave behind.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    export,
    paths::DataDir,
    project::{ProjectStatus, ProjectStore, is_project_id},
    settings::Settings,
};

/// Runs once at startup, before the render scheduler or any import can create files.
pub fn sweep(data: &DataDir) {
    remove_failed_imports(data);
    remove_temp_files(data);
    remove_stale_parts(data);
    crate::render::cache::trim_to(&data.previews(), crate::render::preview::PREVIEW_CAP_BYTES);
}

fn is_temp(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "tmp")
}

/// Deletes the `*.tmp` files directly inside `dir`.
fn remove_temp_files_in(dir: &Path) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_file()) && is_temp(&path) {
            remove_file(&path);
        }
    }
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect()
}

/// Atomic writes that were killed half way leave `*.tmp` files behind.
fn remove_temp_files(data: &DataDir) {
    remove_temp_files_in(data.root());
    for id in project_ids(data) {
        remove_temp_files_in(&data.project(&id));
        for shard in subdirs(&data.project_audio(&id)) {
            remove_temp_files_in(&shard);
        }
    }
    for shard in subdirs(&data.previews()) {
        remove_temp_files_in(&shard);
    }
}

/// `project.json` is written last, so a project directory without one is a failed import.
fn remove_failed_imports(data: &DataDir) {
    for id in project_ids(data) {
        let dir = data.project(&id);
        if dir.is_dir() && !dir.join("project.json").exists() {
            match fs::remove_dir_all(&dir) {
                Ok(()) => tracing::info!("removed unfinished import {}", dir.display()),
                Err(e) => tracing::warn!("couldn't remove {}: {e}", dir.display()),
            }
        }
    }
}

fn project_ids(data: &DataDir) -> Vec<String> {
    let Ok(rd) = fs::read_dir(data.projects()) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|id| is_project_id(id))
        .collect()
}

fn remove_file(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => tracing::info!("removed {}", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("couldn't remove {}: {e}", path.display()),
    }
}

/// Whether `name` is `<stem>.m4b.part` or `<stem> (N).m4b.part` for one of the stems.
fn is_part_of(name: &str, stems: &[String]) -> bool {
    let Some(base) = name.strip_suffix(".m4b.part") else {
        return false;
    };
    stems.iter().any(|stem| {
        base == stem
            || base
                .strip_prefix(stem.as_str())
                .and_then(|rest| rest.strip_prefix(" ("))
                .and_then(|rest| rest.strip_suffix(')'))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// An export that was killed half way leaves `<name>.m4b.part` in the audiobooks folder.
fn remove_stale_parts(data: &DataDir) {
    let Ok(settings) = Settings::load(data) else {
        return;
    };
    let store = ProjectStore::new(data.clone());
    let mut stems = Vec::new();
    for id in project_ids(data) {
        let Ok(p) = store.load(&id) else { continue };
        if let ProjectStatus::Done { output, .. } = &p.status {
            let part: PathBuf = output.with_extension("m4b.part");
            remove_file(&part);
        }
        stems.push(
            export::output_file_name(&p)
                .trim_end_matches(".m4b")
                .to_string(),
        );
    }
    let Ok(rd) = fs::read_dir(&settings.output_dir) else {
        return;
    };
    for entry in rd.flatten() {
        let is_file = entry.file_type().is_ok_and(|t| t.is_file());
        if is_file
            && entry
                .file_name()
                .to_str()
                .is_some_and(|n| is_part_of(n, &stems))
        {
            remove_file(&entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::{Book, BookMeta};

    fn save_project(data: &DataDir, settings: &Settings, id: &str) {
        let book = Book {
            meta: BookMeta {
                title: "Tone".into(),
                authors: vec!["Jane".into()],
                ..Default::default()
            },
            chapters: vec![],
        };
        let mut project = crate::project::Project::new(id, "t.epub".into(), &book, settings);
        ProjectStore::new(data.clone()).save(&mut project).unwrap();
    }

    #[test]
    fn stale_parts_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path().join("data"));
        let out = dir.path().join("out");
        let settings = Settings {
            output_dir: out.clone(),
            ..Settings::default()
        };
        settings.save(&data).unwrap();
        save_project(&data, &settings, "0000000000000001");

        fs::create_dir_all(&out).unwrap();
        for name in [
            "Tone - Jane.m4b.part",
            "Tone - Jane (2).m4b.part",
            "Other.m4b.part",
            "Tone - Jane.m4b",
        ] {
            fs::write(out.join(name), b"x").unwrap();
        }
        sweep(&data);
        assert!(!out.join("Tone - Jane.m4b.part").exists());
        assert!(!out.join("Tone - Jane (2).m4b.part").exists());
        assert!(out.join("Other.m4b.part").exists());
        assert!(out.join("Tone - Jane.m4b").exists());
    }

    #[test]
    fn temp_files_and_failed_imports_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path().join("data"));
        let settings = Settings {
            output_dir: dir.path().join("out"),
            ..Settings::default()
        };
        settings.save(&data).unwrap();
        let id = "0000000000000001";
        save_project(&data, &settings, id);

        let key = "ab".repeat(16);
        let shard = data.project_audio(id).join("ab");
        let preview_shard = data.previews().join("bb");
        let orphan = data.project("0000000000000002");
        fs::create_dir_all(&shard).unwrap();
        fs::create_dir_all(&preview_shard).unwrap();
        fs::create_dir_all(&orphan).unwrap();
        fs::write(orphan.join("book.json"), b"{}").unwrap();
        let temps = [
            data.root().join("settings.json.abc.tmp"),
            data.project(id).join("project.json.x.tmp"),
            shard.join(format!("{key}.pcm.tmp")),
            preview_shard.join(format!("{key}.pcm.tmp")),
        ];
        let keep = [
            data.root().join("settings.json"),
            data.project(id).join("project.json"),
            shard.join(format!("{key}.pcm")),
        ];
        for t in &temps {
            fs::write(t, b"x").unwrap();
        }
        fs::write(&keep[2], b"xx").unwrap();

        sweep(&data);
        for t in &temps {
            assert!(!t.exists(), "{} should be gone", t.display());
        }
        assert!(!orphan.exists());
        for k in &keep {
            assert!(k.exists(), "{} should remain", k.display());
        }
    }
}
