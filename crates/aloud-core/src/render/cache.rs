//! Per-segment PCM cache. One file per speech key; a valid `.pcm` (non-empty, whole samples)
//! means done. This is the whole resume mechanism: there is no other progress state.

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::plan::{Plan, Segment};

pub fn pcm_path(audio_dir: &Path, key: &str) -> PathBuf {
    audio_dir.join(&key[..2]).join(format!("{key}.pcm"))
}

/// A valid entry has at least one whole i16 sample; anything else is a torn or failed write.
fn valid_len(bytes: u64) -> bool {
    bytes >= 2 && bytes.is_multiple_of(2)
}

pub fn exists(audio_dir: &Path, key: &str) -> bool {
    fs::metadata(pcm_path(audio_dir, key)).is_ok_and(|m| m.is_file() && valid_len(m.len()))
}

/// Length in samples of a cached segment (file size / 2).
pub fn len_samples(audio_dir: &Path, key: &str) -> io::Result<u64> {
    let len = fs::metadata(pcm_path(audio_dir, key))?.len();
    if !valid_len(len) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated cached audio",
        ));
    }
    Ok(len / 2)
}

/// Writes mono 24 kHz samples as raw i16 LE via a temp file and rename.
pub fn write_pcm(audio_dir: &Path, key: &str, samples: &[f32]) -> io::Result<()> {
    let path = pcm_path(audio_dir, key);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("pcm.tmp");
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &x in samples {
        let v = (x.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let write = || -> io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.flush()?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &path)
    };
    write().inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Every `<dir>/<xx>/<key>.pcm` with its metadata; unreadable entries are skipped.
fn pcm_files(dir: &Path) -> Vec<(PathBuf, fs::Metadata)> {
    let Ok(shards) = fs::read_dir(dir) else {
        return Vec::new();
    };
    shards
        .flatten()
        .filter_map(|shard| fs::read_dir(shard.path()).ok())
        .flat_map(|files| files.flatten())
        .filter_map(|f| {
            let path = f.path();
            if path.extension()? != "pcm" {
                return None;
            }
            let meta = f.metadata().ok()?;
            meta.is_file().then_some((path, meta))
        })
        .collect()
}

/// Removes cached segments whose key isn't in `keep`; returns how many.
pub fn prune(audio_dir: &Path, keep: &HashSet<&str>) -> u64 {
    let mut removed = 0;
    for (path, _) in pcm_files(audio_dir) {
        let stale = path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|key| !keep.contains(key));
        if stale && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Marks a cached segment as recently used.
pub fn touch(audio_dir: &Path, key: &str) {
    if let Ok(f) = fs::File::options()
        .write(true)
        .open(pcm_path(audio_dir, key))
    {
        let _ = f.set_modified(SystemTime::now());
    }
}

/// Deletes the least recently modified segments until the total is at most `cap` bytes.
pub fn trim_to(dir: &Path, cap: u64) {
    let mut files = pcm_files(dir);
    let mut total: u64 = files.iter().map(|(_, m)| m.len()).sum();
    if total <= cap {
        return;
    }
    files.sort_by_key(|(_, m)| m.modified().unwrap_or(SystemTime::UNIX_EPOCH));
    for (path, meta) in files {
        if total <= cap {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total -= meta.len();
        }
    }
}

pub fn read_pcm(audio_dir: &Path, key: &str) -> io::Result<Vec<f32>> {
    read_pcm_file(&pcm_path(audio_dir, key))
}

/// Reads one `.pcm` cache file (16-bit little-endian) as f32 samples.
pub fn read_pcm_file(path: &Path) -> io::Result<Vec<f32>> {
    let bytes = fs::read(path)?;
    Ok(bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32767.0)
        .collect())
}

/// Σ units of included speech segments that are cached ÷ Σ units of included speech segments.
pub fn cached_fraction(plan: &Plan, included: &HashSet<String>, audio_dir: &Path) -> f32 {
    let mut stat: HashMap<&str, bool> = HashMap::new();
    let (mut done, mut total) = (0u64, 0u64);
    for ch in plan.sequence(included) {
        for seg in ch.segments {
            if let Segment::Speech { key, units, .. } = seg {
                let hit = *stat
                    .entry(key.as_str())
                    .or_insert_with(|| exists(audio_dir, key));
                total += *units as u64;
                if hit {
                    done += *units as u64;
                }
            }
        }
    }
    if total == 0 {
        0.0
    } else {
        done as f32 / total as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_roundtrip_is_lossless_to_one_lsb() {
        let dir = tempfile::tempdir().unwrap();
        let key = "ab".repeat(16);
        let src = [0.0f32, 0.5, -0.5, 1.0, -1.0, 2.0];
        write_pcm(dir.path(), &key, &src).unwrap();
        assert_eq!(len_samples(dir.path(), &key).unwrap(), 6);
        let back = read_pcm(dir.path(), &key).unwrap();
        for (a, b) in src.iter().zip(&back) {
            assert!((a.clamp(-1.0, 1.0) - b).abs() < 1e-4);
        }
    }

    #[test]
    fn empty_and_odd_length_entries_are_missing() {
        let dir = tempfile::tempdir().unwrap();
        let key = "cd".repeat(16);
        write_pcm(dir.path(), &key, &[0.1, 0.2, 0.3]).unwrap();
        assert!(exists(dir.path(), &key));
        let set_len = |n| {
            fs::OpenOptions::new()
                .write(true)
                .open(pcm_path(dir.path(), &key))
                .unwrap()
                .set_len(n)
                .unwrap()
        };
        set_len(3);
        assert!(!exists(dir.path(), &key));
        assert!(len_samples(dir.path(), &key).is_err());
        set_len(0);
        assert!(!exists(dir.path(), &key));
    }

    #[test]
    fn prune_keeps_only_listed_keys() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = ("aa".repeat(16), "bb".repeat(16));
        write_pcm(dir.path(), &a, &[0.1]).unwrap();
        write_pcm(dir.path(), &b, &[0.1]).unwrap();
        let keep: HashSet<&str> = [a.as_str()].into();
        assert_eq!(prune(dir.path(), &keep), 1);
        assert!(exists(dir.path(), &a));
        assert!(!exists(dir.path(), &b));
    }

    #[test]
    fn trim_to_drops_oldest_first() {
        use std::time::{Duration, SystemTime};
        let dir = tempfile::tempdir().unwrap();
        let keys = ["aa", "bb", "cc"].map(|k| k.repeat(16));
        let base = SystemTime::now() - Duration::from_secs(100);
        for (i, key) in keys.iter().enumerate() {
            write_pcm(dir.path(), key, &[0.1; 1000]).unwrap();
            fs::File::options()
                .write(true)
                .open(pcm_path(dir.path(), key))
                .unwrap()
                .set_modified(base + Duration::from_secs(i as u64))
                .unwrap();
        }
        trim_to(dir.path(), 4500);
        assert!(!exists(dir.path(), &keys[0]));
        assert!(exists(dir.path(), &keys[1]));
        assert!(exists(dir.path(), &keys[2]));
    }
}
