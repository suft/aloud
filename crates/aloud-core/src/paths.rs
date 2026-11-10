use std::path::{Path, PathBuf};

/// Root of all persisted Aloud data.
#[derive(Debug, Clone)]
pub struct DataDir(pub PathBuf);

impl DataDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        DataDir(root.into())
    }

    /// `ALOUD_DATA_DIR` if set and non-empty.
    pub fn from_env() -> Option<Self> {
        std::env::var_os("ALOUD_DATA_DIR")
            .filter(|v| !v.is_empty())
            .map(|v| DataDir(PathBuf::from(v)))
    }

    pub fn root(&self) -> &Path {
        &self.0
    }
    pub fn models(&self) -> PathBuf {
        self.0.join("models")
    }
    pub fn settings(&self) -> PathBuf {
        self.0.join("settings.json")
    }
    pub fn queue(&self) -> PathBuf {
        self.0.join("queue.json")
    }
    pub fn projects(&self) -> PathBuf {
        self.0.join("projects")
    }
    pub fn project(&self, id: &str) -> PathBuf {
        self.projects().join(id)
    }
    pub fn project_audio(&self, id: &str) -> PathBuf {
        self.project(id).join("audio")
    }
    pub fn previews(&self) -> PathBuf {
        self.0.join("previews")
    }
}
