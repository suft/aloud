use serde::{Deserialize, Serialize};

use crate::{error::Result, fsx, paths::DataDir};

pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Bookkeeping for the automatic update check, persisted as `update.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdateState {
    /// Unix seconds of the last check that offered nothing; 0 = never.
    pub checked_at: u64,
    /// Version the user chose to skip; automatic checks don't offer it.
    pub skipped_version: Option<String>,
}

impl UpdateState {
    pub fn load(data: &DataDir) -> Result<Self> {
        fsx::read_json_or_default(&data.update_state())
    }

    pub fn save(&self, data: &DataDir) -> Result<()> {
        fsx::write_json_atomic(&data.update_state(), self)?;
        Ok(())
    }

    /// True once the interval has passed, or if the clock went backwards.
    pub fn is_due(&self, now: u64) -> bool {
        now < self.checked_at || now - self.checked_at >= CHECK_INTERVAL_SECS
    }

    pub fn is_skipped(&self, version: &str) -> bool {
        self.skipped_version.as_deref() == Some(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_due_after_interval_or_clock_rewind() {
        let now = 1_700_000_000;
        let at = |checked_at| UpdateState {
            checked_at,
            ..Default::default()
        };
        assert!(UpdateState::default().is_due(now));
        assert!(!at(now - CHECK_INTERVAL_SECS + 1).is_due(now));
        assert!(at(now - CHECK_INTERVAL_SECS).is_due(now));
        assert!(at(now + 10).is_due(now));
    }
}
