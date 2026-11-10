//! Aloud core: EPUB import, Kokoro synthesis, M4B export.
//!
//! The pipeline is `epub::import` → `plan::build_plan` → `render::RenderService`
//! (synthesis into a per-segment PCM cache) → `export::export`.

pub mod book;
pub mod epub;
pub mod error;
pub mod export;
pub mod fsx;
pub mod g2p;
pub mod housekeeping;
pub mod models;
pub mod paths;
pub mod plan;
pub mod project;
pub mod pron;
pub mod queue;
pub mod render;
pub mod settings;
pub mod speed;
pub mod tts;
pub mod update;
pub mod voices;

pub use error::{Error, Result};
pub use paths::DataDir;

/// Sample rate of all audio produced by the model and stored in the cache.
pub const SAMPLE_RATE: u32 = 24_000;
