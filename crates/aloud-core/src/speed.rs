//! Reading-speed limits. The core accepts MIN..=MAX; the app's slider offers UI_MIN..=UI_MAX,
//! the narrower range where Kokoro sounds natural. Both reach the UI through `AppStateDto`.

pub const MIN: f32 = 0.5;
pub const MAX: f32 = 2.0;
pub const UI_MIN: f32 = 0.8;
pub const UI_MAX: f32 = 1.3;

/// Clamps to MIN..=MAX; a non-finite value becomes 1.0.
pub fn clamp(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(MIN, MAX)
    } else {
        1.0
    }
}

#[derive(ts_rs::TS, Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedRange {
    pub min: f32,
    pub max: f32,
    pub ui_min: f32,
    pub ui_max: f32,
}

pub const RANGE: SpeedRange = SpeedRange {
    min: MIN,
    max: MAX,
    ui_min: UI_MIN,
    ui_max: UI_MAX,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_limits_and_sanitizes() {
        assert_eq!(clamp(0.0), 0.5);
        assert_eq!(clamp(-1.0), 0.5);
        assert_eq!(clamp(3.0), 2.0);
        assert_eq!(clamp(1.25), 1.25);
        assert_eq!(clamp(f32::NAN), 1.0);
        assert_eq!(clamp(f32::INFINITY), 1.0);
    }
}
