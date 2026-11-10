//! TypeScript types for the UI, generated from the Rust DTOs (`mise run bindings`).

use std::path::Path;

use aloud_core::{
    book::ChapterKind,
    models::{DownloadProgress, ModelsStatus},
    plan::TrickyWord,
    project::ProjectSummary,
    pron::PronEntry,
    queue::QueueState,
    render::RenderProgress,
    settings::{Performance, Theme},
    voices::VoiceInfo,
};
use ts_rs::{Config, ExportError, TS};

use crate::{
    commands::{
        AboutDoc, AppStateDto, ProjectDetail, ProjectPatch, Scope, SettingsPatch, StorageSize,
        UpdateInfo, UpdateProgress,
    },
    player::PlayerState,
};

/// Writes one `<Name>.ts` per type the UI uses into `dir`, each importing its dependencies.
pub fn export_bindings(dir: &Path) -> Result<(), ExportError> {
    let cfg = Config::new().with_large_int("number").with_out_dir(dir);
    AppStateDto::export_all(&cfg)?;
    ProjectDetail::export_all(&cfg)?;
    ProjectSummary::export_all(&cfg)?;
    VoiceInfo::export_all(&cfg)?;
    TrickyWord::export_all(&cfg)?;
    PronEntry::export_all(&cfg)?;
    QueueState::export_all(&cfg)?;
    RenderProgress::export_all(&cfg)?;
    DownloadProgress::export_all(&cfg)?;
    ModelsStatus::export_all(&cfg)?;
    UpdateInfo::export_all(&cfg)?;
    UpdateProgress::export_all(&cfg)?;
    PlayerState::export_all(&cfg)?;
    StorageSize::export_all(&cfg)?;
    SettingsPatch::export_all(&cfg)?;
    ProjectPatch::export_all(&cfg)?;
    Scope::export_all(&cfg)?;
    AboutDoc::export_all(&cfg)?;
    Performance::export_all(&cfg)?;
    Theme::export_all(&cfg)?;
    ChapterKind::export_all(&cfg)?;
    Ok(())
}
