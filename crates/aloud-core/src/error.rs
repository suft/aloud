use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("This book's data is damaged — remove it and import again ({})", .0.display())]
    Corrupt(PathBuf),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Import(#[from] crate::epub::ImportError),
    #[error("{0}")]
    Download(#[from] crate::models::DownloadError),
    #[error("{0}")]
    Invalid(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn other(msg: impl std::fmt::Display) -> Self {
        Error::Other(msg.to_string())
    }

    /// True when the underlying cause is a full disk.
    pub fn is_storage_full(&self) -> bool {
        matches!(self, Error::Io(e) if e.kind() == std::io::ErrorKind::StorageFull)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
