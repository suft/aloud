use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}", corrupt_message(.0))]
    Corrupt(PathBuf),
    #[error("{}", io_message(.0))]
    Io(#[from] std::io::Error),
    #[error("Some of Aloud's saved data couldn't be read.")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Import(#[from] crate::epub::ImportError),
    #[error("{0}")]
    Download(#[from] crate::models::DownloadError),
    #[error("{0}")]
    Invalid(String),
    #[error("That book couldn't be found. It may have been removed.")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}

/// What to tell the user about an unreadable data file; the path itself isn't useful to them.
fn corrupt_message(path: &Path) -> &'static str {
    match path.file_name().and_then(|n| n.to_str()) {
        Some("settings.json") => "Aloud's settings couldn't be read. Restart Aloud to reset them.",
        Some("queue.json") => {
            "Aloud's list of books to make couldn't be read. Restart Aloud to reset it."
        }
        Some("project.json" | "book.json" | "plan.json") => {
            "This book's data is damaged. Remove it and import it again."
        }
        _ => "Some of Aloud's saved data couldn't be read.",
    }
}

/// Plain-language text for an I/O failure; the raw error goes to the log, not the user.
pub(crate) fn io_message(e: &std::io::Error) -> &'static str {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => "A file Aloud needs is missing. It may have been moved or deleted.",
        PermissionDenied => "Aloud doesn't have permission to use a file or folder it needs.",
        StorageFull => "Your disk is full. Free up some space and try again.",
        ReadOnlyFilesystem => "Aloud can't save there because the drive is read-only.",
        _ => "Aloud couldn't read or write a file it needs.",
    }
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
