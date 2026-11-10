//! Cover storage and accent-color extraction.

use image::{DynamicImage, GenericImageView, ImageFormat, imageops::FilterType};
use std::io::Cursor;

const KEEP_AS_IS_MAX: usize = 2 * 1024 * 1024;
const MAX_SIDE: u32 = 1400;

/// A cover ready to be stored in the project directory.
pub struct Prepared {
    pub name: &'static str,
    pub bytes: Vec<u8>,
    pub accent: Option<String>,
}

/// Decodes the image once. The bytes to store are kept as they are for a JPEG or PNG under 2 MB,
/// otherwise the image is resized to at most 1400 px and saved as JPEG (quality 88).
pub fn prepare(bytes: &[u8]) -> Option<Prepared> {
    let format = image::guess_format(bytes).ok()?;
    let img = image::load_from_memory(bytes).ok()?;
    let accent = accent_of(&img);
    if bytes.len() < KEEP_AS_IS_MAX {
        let name = match format {
            ImageFormat::Jpeg => Some("cover.jpg"),
            ImageFormat::Png => Some("cover.png"),
            _ => None,
        };
        if let Some(name) = name {
            return Some(Prepared {
                name,
                bytes: bytes.to_vec(),
                accent,
            });
        }
    }
    let (w, h) = img.dimensions();
    let img = if w.max(h) > MAX_SIDE {
        img.resize(MAX_SIDE, MAX_SIDE, FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Cursor::new(Vec::new());
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88);
    img.to_rgb8().write_with_encoder(enc).ok()?;
    Some(Prepared {
        name: "cover.jpg",
        bytes: out.into_inner(),
        accent,
    })
}

/// Dominant saturated hue of the image as `#rrggbb`.
fn accent_of(img: &DynamicImage) -> Option<String> {
    let small = img.resize_exact(32, 32, FilterType::Triangle).to_rgb8();

    #[derive(Default, Clone, Copy)]
    struct Bin {
        count: f32,
        sat: f32,
        rgb: [f32; 3],
    }
    let mut bins = [Bin::default(); 12];
    for p in small.pixels() {
        let [r, g, b] = p.0.map(|c| c as f32 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let s = if max == 0.0 { 0.0 } else { d / max };
        let v = max;
        if s < 0.25 || v < 0.2 || v > 0.95 {
            continue;
        }
        let mut h = if max == r {
            ((g - b) / d) % 6.0
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } * 60.0;
        if h < 0.0 {
            h += 360.0;
        }
        let bin = &mut bins[((h / 30.0) as usize).min(11)];
        bin.count += 1.0;
        bin.sat += s;
        bin.rgb[0] += r;
        bin.rgb[1] += g;
        bin.rgb[2] += b;
    }
    let best = bins.iter().filter(|b| b.count > 0.0).max_by(|a, b| {
        let sa = a.count * (a.sat / a.count);
        let sb = b.count * (b.sat / b.count);
        sa.total_cmp(&sb)
    })?;
    let c = best
        .rgb
        .map(|v| ((v / best.count) * 255.0).round().clamp(0.0, 255.0) as u8);
    Some(format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_picks_saturated_color() {
        let img = image::RgbImage::from_fn(40, 40, |x, _| {
            if x < 30 {
                image::Rgb([220, 40, 40])
            } else {
                image::Rgb([128, 128, 128])
            }
        });
        let mut png = Vec::new();
        img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        let p = prepare(&png).unwrap();
        assert_eq!(p.accent.as_deref(), Some("#dc2828"));
        assert_eq!(p.name, "cover.png");
    }
}
