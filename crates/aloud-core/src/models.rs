//! Model files: catalog, presence checks and a resumable, verified downloader.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use crate::{paths::DataDir, settings::ModelVariant};

const BASE_URL: &str =
    "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0";
const CHUNK: usize = 64 * 1024;
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
const SPEED_WINDOW: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
pub struct Asset<'a> {
    pub file: &'a str,
    pub size: u64,
    pub sha256: &'a str,
}

pub const VOICES: Asset<'static> = Asset {
    file: "voices-v1.0.bin",
    size: 28_214_398,
    sha256: "bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d",
};
pub(crate) const STANDARD: Asset<'static> = Asset {
    file: "kokoro-v1.0.onnx",
    size: 325_532_387,
    sha256: "7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5",
};
pub(crate) const FAST: Asset<'static> = Asset {
    file: "kokoro-v1.0.int8.onnx",
    size: 92_361_271,
    sha256: "6e742170d309016e5891a994e1ce1559c702a2ccd0075e67ef7157974f6406cb",
};

impl ModelVariant {
    pub fn asset(self) -> Asset<'static> {
        match self {
            ModelVariant::Standard => STANDARD,
            ModelVariant::Fast => FAST,
        }
    }
    pub fn file(self) -> &'static str {
        self.asset().file
    }
    pub fn size(self) -> u64 {
        self.asset().size
    }
    pub fn sha256(self) -> &'static str {
        self.asset().sha256
    }
}

pub fn model_path(data: &DataDir, variant: ModelVariant) -> PathBuf {
    data.models().join(variant.file())
}

pub fn voices_path(data: &DataDir) -> PathBuf {
    data.models().join(VOICES.file)
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct ModelsStatus {
    pub standard: bool,
    pub fast: bool,
    pub voices: bool,
}

impl ModelsStatus {
    pub fn ready(&self, variant: ModelVariant) -> bool {
        self.voices
            && match variant {
                ModelVariant::Standard => self.standard,
                ModelVariant::Fast => self.fast,
            }
    }
}

fn present(dir: &Path, a: Asset) -> bool {
    fs::metadata(dir.join(a.file)).is_ok_and(|m| m.len() == a.size)
}

/// A file counts as present when it exists with the exact size; hashing happens after download.
pub fn status(data: &DataDir) -> ModelsStatus {
    let dir = data.models();
    ModelsStatus {
        standard: present(&dir, STANDARD),
        fast: present(&dir, FAST),
        voices: present(&dir, VOICES),
    }
}

pub fn remove(data: &DataDir, variant: ModelVariant) -> io::Result<()> {
    let p = model_path(data, variant);
    for path in [p.with_file_name(format!("{}.part", variant.file())), p] {
        match fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub file: String,
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_sec: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("Couldn't reach the download server. Check your internet connection and try again.")]
    Offline,
    #[error("The download was damaged. Please try again.")]
    Corrupt,
    #[error("Not enough disk space.")]
    Disk,
    #[error("The download server answered with an error ({0}). Please try again later.")]
    Http(u16),
    #[error("{0}")]
    Io(io::Error),
    #[error("Cancelled")]
    Cancelled,
}

impl From<io::Error> for DownloadError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::StorageFull {
            DownloadError::Disk
        } else {
            DownloadError::Io(e)
        }
    }
}

/// Downloads whatever is missing for `variant` (voices first, then the model).
pub fn download(
    data: &DataDir,
    variant: ModelVariant,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(DownloadProgress),
) -> Result<(), DownloadError> {
    let dir = data.models();
    fs::create_dir_all(&dir)?;
    for asset in [VOICES, variant.asset()] {
        fetch(BASE_URL, asset, &dir, cancel, on_progress, PROGRESS_EVERY)?;
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_file(path: &Path, hasher: &mut Sha256) -> io::Result<()> {
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        hasher.update(&buf[..n]);
    }
}

struct SpeedMeter {
    samples: VecDeque<(Instant, u64)>,
}

impl SpeedMeter {
    fn record(&mut self, downloaded: u64) -> f64 {
        let now = Instant::now();
        self.samples.push_back((now, downloaded));
        while self
            .samples
            .front()
            .is_some_and(|(t, _)| now.duration_since(*t) > SPEED_WINDOW)
        {
            self.samples.pop_front();
        }
        match (self.samples.front(), self.samples.back()) {
            (Some((t0, b0)), Some((t1, b1))) if t1 > t0 => {
                (b1 - b0) as f64 / t1.duration_since(*t0).as_secs_f64()
            }
            _ => 0.0,
        }
    }
}

/// Downloads one asset to `dir`, resuming from `<file>.part`, verifying size and sha256.
pub(crate) fn fetch(
    base_url: &str,
    asset: Asset,
    dir: &Path,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(DownloadProgress),
    progress_every: Duration,
) -> Result<(), DownloadError> {
    let final_path = dir.join(asset.file);
    if present(dir, asset) {
        return Ok(());
    }
    let part = dir.join(format!("{}.part", asset.file));
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .map_err(|_| DownloadError::Offline)?;

    let mut restarted = false;
    loop {
        let mut existing = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if existing > asset.size {
            fs::remove_file(&part)?;
            existing = 0;
        }
        let mut hasher = Sha256::new();
        if existing > 0 {
            hash_file(&part, &mut hasher)?;
        }

        let mut req = client.get(format!("{base_url}/{}", asset.file));
        if existing > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
        }
        let mut resp = req.send().map_err(|_| DownloadError::Offline)?;
        let status = resp.status().as_u16();
        if status == 416 && !restarted {
            // The partial file doesn't match the server's idea of the file; start over.
            fs::remove_file(&part)?;
            restarted = true;
            continue;
        }
        if status != 200 && status != 206 {
            return Err(DownloadError::Http(status));
        }
        let mut file = if status == 206 {
            OpenOptions::new().append(true).open(&part)?
        } else {
            // The server ignored the range: restart from zero.
            existing = 0;
            hasher = Sha256::new();
            File::create(&part)?
        };

        let mut downloaded = existing;
        let mut meter = SpeedMeter {
            samples: VecDeque::new(),
        };
        let mut last_report: Option<Instant> = None;
        let mut buf = vec![0u8; CHUNK];
        let report = |downloaded: u64, meter: &mut SpeedMeter| {
            on_progress(DownloadProgress {
                file: asset.file.to_string(),
                downloaded,
                total: asset.size,
                bytes_per_sec: meter.record(downloaded),
            });
        };
        loop {
            if cancel.load(Ordering::Relaxed) {
                file.flush()?;
                return Err(DownloadError::Cancelled);
            }
            let n = match resp.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(DownloadError::Offline),
            };
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            downloaded += n as u64;
            if last_report.is_none_or(|t| t.elapsed() >= progress_every) {
                last_report = Some(Instant::now());
                report(downloaded, &mut meter);
            }
        }
        file.flush()?;
        file.sync_all()?;
        drop(file);
        report(downloaded, &mut meter);

        if downloaded != asset.size || hex(&hasher.finalize()) != asset.sha256 {
            let _ = fs::remove_file(&part);
            return Err(DownloadError::Corrupt);
        }
        fs::rename(&part, &final_path)?;
        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn blob() -> Vec<u8> {
        (0..1024 * 1024u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 24) as u8)
            .collect()
    }

    /// Serves `/blob.bin` with Range support; returns base URL and the Range headers seen.
    fn serve(data: Vec<u8>) -> (String, Arc<Mutex<Vec<String>>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}",
            server.server_addr().to_ip().unwrap().port()
        );
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let seen = ranges.clone();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let range = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Range"))
                    .map(|h| h.value.to_string());
                let resp = match range.as_deref().and_then(|r| r.strip_prefix("bytes=")) {
                    Some(r) => {
                        seen.lock().unwrap().push(r.to_string());
                        let start: usize = r.trim_end_matches('-').parse().unwrap();
                        tiny_http::Response::from_data(data[start..].to_vec()).with_status_code(206)
                    }
                    None => tiny_http::Response::from_data(data.clone()).with_status_code(200),
                };
                let _ = req.respond(resp);
            }
        });
        (url, ranges)
    }

    fn asset<'a>(data: &[u8], sha: &'a str) -> Asset<'a> {
        Asset {
            file: "blob.bin",
            size: data.len() as u64,
            sha256: sha,
        }
    }

    #[test]
    fn cancel_keeps_part_and_resume_uses_range() {
        let data = blob();
        let sha = hex(&Sha256::digest(&data));
        let (url, ranges) = serve(data.clone());
        let dir = tempfile::tempdir().unwrap();
        let a = asset(&data, &sha);

        let cancel = AtomicBool::new(false);
        let r = fetch(
            &url,
            a,
            dir.path(),
            &cancel,
            &|p| {
                if p.downloaded >= p.total / 2 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            Duration::ZERO,
        );
        assert!(matches!(r, Err(DownloadError::Cancelled)));
        let part_len = fs::metadata(dir.path().join("blob.bin.part"))
            .unwrap()
            .len();
        assert!(part_len >= data.len() as u64 / 2 && part_len < data.len() as u64);
        assert!(!dir.path().join("blob.bin").exists());

        cancel.store(false, Ordering::Relaxed);
        fetch(&url, a, dir.path(), &cancel, &|_| {}, Duration::ZERO).unwrap();
        assert_eq!(ranges.lock().unwrap().as_slice(), [format!("{part_len}-")]);
        assert_eq!(fs::read(dir.path().join("blob.bin")).unwrap(), data);
        assert!(!dir.path().join("blob.bin.part").exists());
    }

    #[test]
    fn wrong_hash_is_corrupt_and_leaves_nothing() {
        let data = blob();
        let (url, _) = serve(data.clone());
        let dir = tempfile::tempdir().unwrap();
        let zeros = "0".repeat(64);
        let a = asset(&data, &zeros);
        let r = fetch(
            &url,
            a,
            dir.path(),
            &AtomicBool::new(false),
            &|_| {},
            Duration::ZERO,
        );
        assert!(matches!(r, Err(DownloadError::Corrupt)));
        assert!(!dir.path().join("blob.bin").exists());
        assert!(!dir.path().join("blob.bin.part").exists());
    }
}
