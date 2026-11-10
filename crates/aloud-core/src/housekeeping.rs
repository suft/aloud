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
    remove_stale_parts(data);
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
        let book = Book {
            meta: BookMeta {
                title: "Tone".into(),
                authors: vec!["Jane".into()],
                ..Default::default()
            },
            chapters: vec![],
        };
        let mut project =
            crate::project::Project::new("0000000000000001", "t.epub".into(), &book, &settings);
        ProjectStore::new(data.clone()).save(&mut project).unwrap();

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
}
