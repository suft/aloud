use serde::Serialize;

/// Error returned to the UI as `{ message }`; the text is meant for non-developers.
#[derive(Debug, Serialize)]
pub struct AppError {
    pub message: String,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        AppError {
            message: message.into(),
        }
    }
}

impl From<aloud_core::Error> for AppError {
    fn from(e: aloud_core::Error) -> Self {
        log::warn!("{e:?}");
        AppError::new(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        aloud_core::Error::from(e).into()
    }
}

impl From<aloud_core::models::DownloadError> for AppError {
    fn from(e: aloud_core::models::DownloadError) -> Self {
        AppError::new(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// Runs blocking core work off the async runtime.
pub async fn run<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::new(format!("Something went wrong: {e}")))?
}
