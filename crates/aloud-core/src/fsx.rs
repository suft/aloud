use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
};

use crate::error::{Error, Result};

/// Serializes `value` to `path` via a synced temp file and an atomic rename.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    write_atomic(path, &serde_json::to_vec(value).map_err(io::Error::other)?)
}

/// Writes `bytes` to `path` via a synced, uniquely named temp file and an atomic rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let tmp = path.with_extension(format!("{ext}.{}.tmp", uuid::Uuid::new_v4().simple()));
    let write = || -> io::Result<()> {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    };
    write().inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Reads JSON from `path`. A missing file yields `None`; a corrupt file is `Error::Corrupt`.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| Error::Corrupt(path.to_path_buf()))
}

/// Reads JSON, treating a missing file as `T::default()`.
pub fn read_json_or_default<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    Ok(read_json(path)?.unwrap_or_default())
}

/// Renames an unreadable JSON file to `<name>.bak` (replacing an older backup) so the caller
/// falls back to defaults. Returns true when it did. Missing or valid files are left alone.
pub fn set_aside_if_corrupt<T: DeserializeOwned>(path: &Path) -> io::Result<bool> {
    if let Err(Error::Corrupt(_)) = read_json::<T>(path) {
        fs::rename(path, path.with_extension("json.bak"))?;
        return Ok(true);
    }
    Ok(false)
}

/// Total size in bytes of all files below `dir` (0 when it does not exist).
pub fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_roundtrip_and_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a/b.json");
        assert_eq!(read_json::<Vec<u8>>(&p).unwrap(), None);
        write_json_atomic(&p, &vec![1u8, 2, 3]).unwrap();
        assert_eq!(read_json::<Vec<u8>>(&p).unwrap(), Some(vec![1, 2, 3]));
        fs::write(&p, b"{nope").unwrap();
        assert!(matches!(read_json::<Vec<u8>>(&p), Err(Error::Corrupt(_))));
    }

    #[test]
    fn set_aside_keeps_the_corrupt_file_as_bak() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        assert!(!set_aside_if_corrupt::<Vec<u8>>(&p).unwrap());
        fs::write(&p, b"garbage").unwrap();
        assert!(set_aside_if_corrupt::<Vec<u8>>(&p).unwrap());
        assert!(!p.exists());
        assert_eq!(
            fs::read(dir.path().join("settings.json.bak")).unwrap(),
            b"garbage"
        );
        write_json_atomic(&p, &vec![1u8]).unwrap();
        assert!(!set_aside_if_corrupt::<Vec<u8>>(&p).unwrap());
        assert!(p.exists());
    }
}
