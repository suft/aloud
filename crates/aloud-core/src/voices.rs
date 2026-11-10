//! Voice catalog and style-vector loading from `voices-v1.0.bin` (an NPZ of `<name>.npy`).

use serde::Serialize;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};
use zip::ZipArchive;

use crate::error::{Error, Result};

const STYLE_ROWS: usize = 510;
const STYLE_DIM: usize = 256;

#[derive(ts_rs::TS, Debug, Clone, Serialize)]
pub struct VoiceInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// `"us"` or `"uk"`.
    #[ts(type = "\"us\" | \"uk\"")]
    pub accent: &'static str,
    /// `"female"` or `"male"`.
    #[ts(type = "\"female\" | \"male\"")]
    pub gender: &'static str,
    pub grade: &'static str,
    pub recommended: bool,
}

const fn v(
    id: &'static str,
    name: &'static str,
    accent: &'static str,
    gender: &'static str,
    grade: &'static str,
    recommended: bool,
) -> VoiceInfo {
    VoiceInfo {
        id,
        name,
        accent,
        gender,
        grade,
        recommended,
    }
}

/// English voices with the grades published in hexgrad's VOICES.md.
pub static CATALOG: [VoiceInfo; 28] = [
    v("af_heart", "Heart", "us", "female", "A", true),
    v("af_bella", "Bella", "us", "female", "A-", true),
    v("af_nicole", "Nicole", "us", "female", "B-", true),
    v("af_aoede", "Aoede", "us", "female", "C+", false),
    v("af_kore", "Kore", "us", "female", "C+", false),
    v("af_sarah", "Sarah", "us", "female", "C+", false),
    v("af_alloy", "Alloy", "us", "female", "C", false),
    v("af_nova", "Nova", "us", "female", "C", false),
    v("af_sky", "Sky", "us", "female", "C-", false),
    v("af_jessica", "Jessica", "us", "female", "D", false),
    v("af_river", "River", "us", "female", "D", false),
    v("am_fenrir", "Fenrir", "us", "male", "C+", false),
    v("am_michael", "Michael", "us", "male", "C+", false),
    v("am_puck", "Puck", "us", "male", "C+", false),
    v("am_echo", "Echo", "us", "male", "D", false),
    v("am_eric", "Eric", "us", "male", "D", false),
    v("am_liam", "Liam", "us", "male", "D", false),
    v("am_onyx", "Onyx", "us", "male", "D", false),
    v("am_santa", "Santa", "us", "male", "D-", false),
    v("am_adam", "Adam", "us", "male", "F+", false),
    v("bf_emma", "Emma", "uk", "female", "B-", true),
    v("bf_isabella", "Isabella", "uk", "female", "C", false),
    v("bm_fable", "Fable", "uk", "male", "C", false),
    v("bm_george", "George", "uk", "male", "C", false),
    v("bm_lewis", "Lewis", "uk", "male", "D+", false),
    v("bf_alice", "Alice", "uk", "female", "D", false),
    v("bf_lily", "Lily", "uk", "female", "D", false),
    v("bm_daniel", "Daniel", "uk", "male", "D", false),
];

pub fn is_known(voice: &str) -> bool {
    CATALOG.iter().any(|c| c.id == voice)
}

pub fn info(voice: &str) -> Option<&'static VoiceInfo> {
    CATALOG.iter().find(|c| c.id == voice)
}

pub struct Voices {
    styles: HashMap<String, Box<[f32]>>,
}

fn bad(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("The voice file is damaged ({msg})."))
}

/// Parses an `.npy` v1 file holding little-endian f32 data of shape `(510, 1, 256)`.
fn parse_npy(r: &mut impl Read) -> Result<Box<[f32]>> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic[..6] != b"\x93NUMPY" || magic[6] != 1 {
        return Err(bad("bad npy header"));
    }
    let mut len = [0u8; 2];
    r.read_exact(&mut len)?;
    let mut header = vec![0u8; u16::from_le_bytes(len) as usize];
    r.read_exact(&mut header)?;
    let header = String::from_utf8_lossy(&header);
    if !header.contains("'<f4'")
        || !header.contains("(510, 1, 256)")
        || !header.contains("'fortran_order': False")
    {
        return Err(bad("unexpected array layout"));
    }
    let mut raw = vec![0u8; STYLE_ROWS * STYLE_DIM * 4];
    r.read_exact(&mut raw)?;
    Ok(raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

impl Voices {
    pub fn load(path: &Path) -> Result<Self> {
        let mut zip = ZipArchive::new(BufReader::new(File::open(path)?)).map_err(bad)?;
        let mut styles = HashMap::new();
        for c in &CATALOG {
            let mut f = match zip.by_name(&format!("{}.npy", c.id)) {
                Ok(f) => f,
                Err(zip::result::ZipError::FileNotFound) => continue,
                Err(e) => return Err(bad(e)),
            };
            styles.insert(c.id.to_string(), parse_npy(&mut f)?);
        }
        if styles.is_empty() {
            return Err(bad("no voices found"));
        }
        Ok(Voices { styles })
    }

    pub fn has(&self, voice: &str) -> bool {
        self.styles.contains_key(voice)
    }

    /// Style row for an utterance of `n_ids` phoneme ids (pads excluded).
    pub fn style(&self, voice: &str, n_ids: usize) -> Option<&[f32]> {
        let all = self.styles.get(voice)?;
        let row = n_ids.clamp(1, STYLE_ROWS) - 1;
        Some(&all[row * STYLE_DIM..(row + 1) * STYLE_DIM])
    }
}
