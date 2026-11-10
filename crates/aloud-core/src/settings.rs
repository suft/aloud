use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{error::Result, fsx, paths::DataDir, pron::PronEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelVariant {
    #[default]
    Standard,
    Fast,
}

impl ModelVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelVariant::Standard => "standard",
            ModelVariant::Fast => "fast",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Performance {
    #[default]
    Full,
    Balanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub model: ModelVariant,
    pub output_dir: PathBuf,
    pub performance: Performance,
    pub gpu: bool,
    pub keep_awake: bool,
    pub notify_on_finish: bool,
    pub check_updates: bool,
    pub default_voice: String,
    pub default_speed: f32,
    pub pronunciations: Vec<PronEntry>,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        let output_dir = dirs::audio_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Audiobooks");
        Settings {
            model: ModelVariant::Standard,
            output_dir,
            performance: Performance::Full,
            gpu: false,
            keep_awake: true,
            notify_on_finish: true,
            check_updates: true,
            default_voice: "af_heart".into(),
            default_speed: 1.0,
            pronunciations: Vec::new(),
            theme: Theme::System,
        }
    }
}

impl Settings {
    pub fn load(data: &DataDir) -> Result<Self> {
        fsx::read_json_or_default(&data.settings())
    }

    pub fn save(&self, data: &DataDir) -> Result<()> {
        fsx::write_json_atomic(&data.settings(), self)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_defaults() {
        let s: Settings = serde_json::from_str(r#"{"model":"fast","gpu":true}"#).unwrap();
        assert_eq!(s.model, ModelVariant::Fast);
        assert!(s.check_updates);
        assert!(s.gpu);
        assert!(s.keep_awake);
        assert_eq!(s.default_voice, "af_heart");
    }
}
