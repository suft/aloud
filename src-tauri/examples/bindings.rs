//! Regenerates `src/lib/bindings` from the Rust DTOs, or with `--check` verifies it is up to date.

use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

fn read_files(dir: &Path) -> std::io::Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        files.insert(
            entry.file_name().to_string_lossy().into_owned(),
            fs::read(entry.path())?,
        );
    }
    Ok(files)
}

fn run(check: bool) -> Result<bool, Box<dyn Error>> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings");
    if !check {
        if out.exists() {
            fs::remove_dir_all(&out)?;
        }
        aloud_lib::export_bindings(&out)?;
        return Ok(true);
    }
    let tmp = std::env::temp_dir().join(format!("aloud-bindings-{}", std::process::id()));
    let result = aloud_lib::export_bindings(&tmp)
        .map_err(Box::<dyn Error>::from)
        .and_then(|()| {
            let fresh = read_files(&tmp)?;
            Ok(read_files(&out).is_ok_and(|current| current == fresh))
        });
    let _ = fs::remove_dir_all(&tmp);
    result
}

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    match run(check) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!("src/lib/bindings is out of date; run `mise run bindings`");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("bindings: {e}");
            ExitCode::FAILURE
        }
    }
}
