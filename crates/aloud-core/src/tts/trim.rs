/// Port of `librosa.effects.trim` (top_db = 60, frame 2048, hop 512) as used by kokoro-onnx.
const FRAME: usize = 2048;
const HOP: usize = 512;
const TOP_DB: f32 = 60.0;
const AMIN: f32 = 1e-5;

/// Returns the slice of `audio` between the first and last non-silent frames.
pub fn trim(audio: &[f32]) -> &[f32] {
    if audio.is_empty() {
        return audio;
    }
    // Centered framing: zero-pad FRAME/2 on both sides.
    let pad = FRAME / 2;
    let frames = 1 + audio.len() / HOP;
    let sample = |i: usize| -> f32 {
        // index into the padded signal
        if i < pad || i - pad >= audio.len() {
            0.0
        } else {
            audio[i - pad]
        }
    };
    let mut rms = Vec::with_capacity(frames);
    for f in 0..frames {
        let start = f * HOP;
        let mut acc = 0.0f64;
        for i in start..start + FRAME {
            let s = sample(i) as f64;
            acc += s * s;
        }
        rms.push((acc / FRAME as f64).sqrt() as f32);
    }
    let max_rms = rms.iter().copied().fold(0.0f32, f32::max).max(AMIN);
    let non_silent = |r: f32| 20.0 * (r.max(AMIN) / max_rms).log10() > -TOP_DB;
    let first = rms.iter().position(|&r| non_silent(r));
    let last = rms.iter().rposition(|&r| non_silent(r));
    match (first, last) {
        (Some(f), Some(l)) => &audio[f * HOP..audio.len().min((l + 1) * HOP)],
        _ => &audio[0..0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_silence_around_a_tone() {
        let mut a = vec![0.0f32; 8000];
        a.extend((0..4000).map(|i| (i as f32 * 0.2).sin() * 0.5));
        a.extend(vec![0.0f32; 8000]);
        let t = trim(&a);
        assert!(t.len() < a.len() - 8000, "len {}", t.len());
        assert!(t.len() >= 4000);
    }
}
