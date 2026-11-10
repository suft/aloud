use serde::{Deserialize, Serialize};

use crate::{error::Result, fsx, paths::DataDir};

/// Persisted render queue. The head is the active (or next) project.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QueueState {
    pub items: Vec<String>,
    pub paused: bool,
}

impl QueueState {
    pub fn load(data: &DataDir) -> Result<Self> {
        fsx::read_json_or_default(&data.queue())
    }

    pub fn save(&self, data: &DataDir) -> Result<()> {
        fsx::write_json_atomic(&data.queue(), self)?;
        Ok(())
    }
}
