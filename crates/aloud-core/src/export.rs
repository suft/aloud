//! Cached segments → loudness-normalized, chaptered `.m4b`.

use ebur128::{EbuR128, Mode};
use mp4::{
    AacConfig, AudioObjectType, Bytes, ChannelConfig, FourCC, MediaConfig, Mp4Config, Mp4Sample,
    Mp4Writer, SampleFreqIndex, TrackConfig, TrackType,
};
use mp4ameta::{Chapter, Img, MediaType, Tag};
use rusty_aac::{AacEncoder, AacEncoderConfig};
use std::{
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    SAMPLE_RATE,
    error::{Error, Result},
    paths::DataDir,
    plan::{Plan, Segment},
    project::{Project, ProjectStatus},
    render::cache,
    settings::Settings,
    voices,
};

const TARGET_LUFS: f64 = -18.0;
const MAX_TRUE_PEAK_DB: f64 = -1.5;
const BITRATE: u32 = 80_000;
/// Flush the PCM buffer to the encoder once it holds at least this many samples (10 minutes).
const FLUSH_SAMPLES: usize = 10 * 60 * SAMPLE_RATE as usize;
const SILENCE_CHUNK: usize = SAMPLE_RATE as usize;

#[derive(Debug, Clone)]
pub struct FinishedInfo {
    pub output: PathBuf,
    pub duration_secs: u64,
}

fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    trimmed
        .chars()
        .take(150)
        .collect::<String>()
        .trim()
        .to_string()
}

/// `<Title> - <First author>.m4b`
pub fn output_file_name(project: &Project) -> String {
    let mut base = sanitize(&project.meta.title);
    if let Some(a) = project.meta.authors.first() {
        let a = sanitize(a);
        if !a.is_empty() {
            base = format!("{base} - {a}");
        }
    }
    if base.is_empty() {
        base = "Audiobook".into();
    }
    format!("{}.m4b", sanitize(&base))
}

fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let first = dir.join(file_name);
    if !first.exists() {
        return first;
    }
    let stem = file_name.trim_end_matches(".m4b");
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).m4b")))
        .find(|p| !p.exists())
        .expect("infinite iterator")
}

fn segment_samples(audio_dir: &Path, seg: &Segment) -> Result<u64> {
    match seg {
        Segment::Speech { key, .. } => cache::len_samples(audio_dir, key).map_err(|_| {
            Error::other("Some of the saved audio is missing. Press Continue to make it again.")
        }),
        Segment::Silence { ms } => Ok(*ms as u64 * SAMPLE_RATE as u64 / 1000),
    }
}

fn silence_len(ms: u32) -> usize {
    (ms as u64 * SAMPLE_RATE as u64 / 1000) as usize
}

fn missing_audio(_: std::io::Error) -> Error {
    Error::other("Some of the saved audio is missing. Press Continue to make it again.")
}

/// Gain in linear units that brings the audio to the target loudness without clipping.
fn measure_gain(audio_dir: &Path, chapters: &[crate::plan::SeqChapter]) -> Result<f32> {
    let mut meter =
        EbuR128::new(1, SAMPLE_RATE, Mode::I | Mode::TRUE_PEAK).map_err(Error::other)?;
    let zeros = vec![0.0f32; SILENCE_CHUNK];
    for ch in chapters {
        for seg in &ch.segments {
            match seg {
                Segment::Speech { key, .. } => {
                    let pcm = cache::read_pcm(audio_dir, key).map_err(missing_audio)?;
                    meter.add_frames_f32(&pcm).map_err(Error::other)?;
                }
                Segment::Silence { ms } => {
                    let mut left = silence_len(*ms);
                    while left > 0 {
                        let n = left.min(SILENCE_CHUNK);
                        meter.add_frames_f32(&zeros[..n]).map_err(Error::other)?;
                        left -= n;
                    }
                }
            }
        }
    }
    let integrated = meter.loudness_global().map_err(Error::other)?;
    if !integrated.is_finite() {
        return Ok(1.0);
    }
    let peak = meter.true_peak(0).map_err(Error::other)?;
    let mut gain_db = TARGET_LUFS - integrated;
    if peak > 0.0 {
        let peak_db = 20.0 * peak.log10();
        if peak_db + gain_db > MAX_TRUE_PEAK_DB {
            gain_db = MAX_TRUE_PEAK_DB - peak_db;
        }
    }
    Ok(10f64.powf(gain_db / 20.0) as f32)
}

struct Muxer {
    writer: Mp4Writer<BufWriter<File>>,
    samples_written: u64,
}

impl Muxer {
    /// Encodes `buf` as a standalone AAC stream and appends its packets to the track.
    fn flush(&mut self, buf: &mut Vec<f32>) -> Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        let mut enc = AacEncoder::new(AacEncoderConfig {
            bitrate_bps: BITRATE,
            ..Default::default()
        });
        enc.push_pcm(buf, 1, SAMPLE_RATE).map_err(Error::other)?;
        enc.finish();
        loop {
            match enc.next_packet() {
                Ok(p) => {
                    self.writer
                        .write_sample(
                            1,
                            &Mp4Sample {
                                start_time: self.samples_written,
                                duration: 1024,
                                rendering_offset: 0,
                                is_sync: true,
                                bytes: Bytes::from(p.data),
                            },
                        )
                        .map_err(Error::other)?;
                    self.samples_written += 1024;
                }
                Err(rusty_aac::Error::Eof) => break,
                Err(e) => return Err(Error::other(e)),
            }
        }
        buf.clear();
        Ok(())
    }
}

pub fn export(
    data: &DataDir,
    project: &Project,
    plan: &Plan,
    settings: &Settings,
    progress: &dyn Fn(f32),
) -> Result<FinishedInfo> {
    let audio_dir = data.project_audio(&project.id);
    let included = project.included_ids();
    let chapters = plan.sequence(&included);
    if chapters.is_empty() {
        return Err(Error::Invalid("Select at least one chapter first.".into()));
    }

    let out_path = match &project.status {
        ProjectStatus::Done { output, .. } if output.exists() => output.clone(),
        _ => {
            fs::create_dir_all(&settings.output_dir)?;
            unique_path(&settings.output_dir, &output_file_name(project))
        }
    };
    if let Some(dir) = out_path.parent() {
        fs::create_dir_all(dir)?;
    }
    let part = out_path.with_extension("m4b.part");

    let result = write_m4b(
        data, project, &chapters, &audio_dir, settings, &part, progress,
    );
    match result {
        Ok(info) => {
            fs::rename(&part, &out_path)?;
            Ok(FinishedInfo {
                output: out_path,
                ..info
            })
        }
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

fn write_m4b(
    data: &DataDir,
    project: &Project,
    chapters: &[crate::plan::SeqChapter],
    audio_dir: &Path,
    _settings: &Settings,
    part: &Path,
    progress: &dyn Fn(f32),
) -> Result<FinishedInfo> {
    let mut total_samples = 0u64;
    for ch in chapters {
        for seg in &ch.segments {
            total_samples += segment_samples(audio_dir, seg)?;
        }
    }

    let gain = measure_gain(audio_dir, chapters)?;

    let file = BufWriter::new(File::create(part)?);
    let cfg = Mp4Config {
        major_brand: FourCC::from(*b"M4B "),
        minor_version: 512,
        compatible_brands: vec![
            FourCC::from(*b"M4B "),
            FourCC::from(*b"M4A "),
            FourCC::from(*b"mp42"),
            FourCC::from(*b"isom"),
        ],
        timescale: 1000,
    };
    let mut mux = Muxer {
        writer: Mp4Writer::write_start(file, &cfg).map_err(Error::other)?,
        samples_written: 0,
    };
    mux.writer
        .add_track(&TrackConfig {
            track_type: TrackType::Audio,
            timescale: SAMPLE_RATE,
            language: "eng".into(),
            media_conf: MediaConfig::AacConfig(AacConfig {
                bitrate: BITRATE,
                profile: AudioObjectType::AacLowComplexity,
                freq_index: SampleFreqIndex::Freq24000,
                chan_conf: ChannelConfig::Mono,
            }),
        })
        .map_err(Error::other)?;

    let mut starts: Vec<(u64, String)> = Vec::with_capacity(chapters.len());
    let mut buf: Vec<f32> = Vec::new();
    let mut fed = 0u64;
    for ch in chapters {
        starts.push((mux.samples_written, ch.title.to_string()));
        for seg in &ch.segments {
            match seg {
                Segment::Speech { key, .. } => {
                    let pcm = cache::read_pcm(audio_dir, key).map_err(missing_audio)?;
                    fed += pcm.len() as u64;
                    buf.extend(pcm.iter().map(|x| (x * gain).clamp(-1.0, 1.0)));
                }
                Segment::Silence { ms } => {
                    let n = silence_len(*ms);
                    fed += n as u64;
                    buf.resize(buf.len() + n, 0.0);
                    if buf.len() >= FLUSH_SAMPLES {
                        mux.flush(&mut buf)?;
                    }
                }
            }
            progress(fed as f32 / total_samples.max(1) as f32);
        }
        mux.flush(&mut buf)?;
    }
    let duration_secs = mux.samples_written / SAMPLE_RATE as u64;
    mux.writer.write_end().map_err(Error::other)?;
    drop(mux);
    fix_esds(part)?;

    write_tags(data, project, part, &starts)?;
    Ok(FinishedInfo {
        output: part.to_path_buf(),
        duration_secs,
    })
}

/// The `mp4` crate writes the esds SLConfig descriptor with length 0 although it carries a
/// one-byte payload, which strict parsers reject. Rewrites it as the standard `06 01 02`.
fn fix_esds(path: &Path) -> std::io::Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    const CHUNK: u64 = 4 * 1024 * 1024;
    const OVERLAP: u64 = 64;
    let mut f = fs::OpenOptions::new().read(true).write(true).open(path)?;
    let len = f.metadata()?.len();
    let mut end = len;
    let mut buf = Vec::new();
    // moov sits at the end of the file, so scan backwards.
    while end > 0 {
        let start = end.saturating_sub(CHUNK);
        let read_end = (end + OVERLAP).min(len);
        buf.resize((read_end - start) as usize, 0);
        f.seek(SeekFrom::Start(start))?;
        f.read_exact(&mut buf)?;
        if let Some(i) = buf.windows(4).rposition(|w| w == b"esds") {
            let found = buf[i..]
                .windows(7)
                .position(|w| w[0] == 5 && w[1] == 2 && w[4..7] == [6, 0, 0]);
            if let Some(j) = found {
                f.seek(SeekFrom::Start(start + (i + j + 4) as u64))?;
                f.write_all(&[6, 1, 2])?;
            }
            return Ok(());
        }
        end = start;
    }
    Ok(())
}

fn write_tags(
    data: &DataDir,
    project: &Project,
    part: &Path,
    starts: &[(u64, String)],
) -> Result<()> {
    let mut tag = Tag::read_from_path(part).map_err(Error::other)?;
    let meta = &project.meta;
    let authors = meta.authors.join(", ");
    tag.set_title(meta.title.clone());
    tag.set_album(meta.title.clone());
    if !authors.is_empty() {
        tag.set_artist(authors.clone());
        tag.set_album_artist(authors);
    }
    tag.set_genre("Audiobook");
    if let Some(year) = meta
        .date
        .as_ref()
        .map(|d| d.chars().take(4).collect::<String>())
        && year.len() == 4
        && year.chars().all(|c| c.is_ascii_digit())
    {
        tag.set_year(year);
    }
    if let Some(d) = &meta.description {
        tag.set_description(d.clone());
    }
    let voice_name = voices::info(&project.voice).map_or(project.voice.as_str(), |v| v.name);
    tag.set_comment(format!("Narrated by {voice_name} (Kokoro) with Aloud"));
    tag.set_media_type(MediaType::AudioBook);
    if let Some(cover) = project.cover_path(data)
        && let Ok(bytes) = fs::read(&cover)
    {
        if cover.extension().is_some_and(|e| e == "png") {
            tag.set_artwork(Img::png(bytes));
        } else {
            tag.set_artwork(Img::jpeg(bytes));
        }
    }
    for (start, title) in starts {
        let at = Duration::from_secs_f64(*start as f64 / SAMPLE_RATE as f64);
        tag.chapter_list_mut().push(Chapter::new(at, title.clone()));
        tag.chapter_track_mut()
            .push(Chapter::new(at, title.clone()));
    }
    tag.write_to_path(part).map_err(Error::other)?;
    Ok(())
}

#[cfg(test)]
mod tests;
