use aloud_core::{
    DataDir,
    project::ProjectStore,
    queue::QueueState,
    render::{Events, FinishedInfo, RenderProgress, RenderService},
    settings::Settings,
};
use std::sync::{Arc, Mutex, atomic::AtomicBool};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

use crate::player::Player;

pub struct AppState {
    pub data: DataDir,
    pub store: ProjectStore,
    pub render: Arc<RenderService>,
    pub player: Player,
    pub app: AppHandle,
    /// Serializes read-modify-write of `settings.json`.
    pub settings_lock: Mutex<()>,
    pub download_cancel: AtomicBool,
    pub downloading: AtomicBool,
    /// Update found by the last check, kept so it can be installed without re-checking.
    pub pending_update: Mutex<Option<tauri_plugin_updater::Update>>,
    /// Codes for things that happened before the UI loaded; shown once by `get_app_state`.
    pub startup_notices: Mutex<Vec<&'static str>>,
}

impl AppState {
    pub fn settings(&self) -> Settings {
        Settings::load(&self.data).unwrap_or_default()
    }

    /// Applies `f` to the persisted settings and saves them, unless `f` fails.
    pub fn update_settings<R, E: From<aloud_core::Error>>(
        &self,
        f: impl FnOnce(&mut Settings) -> Result<R, E>,
    ) -> Result<(Settings, R), E> {
        let _guard = self.settings_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = Settings::load(&self.data)?;
        let r = f(&mut s)?;
        s.save(&self.data)?;
        Ok((s, r))
    }

    pub fn emit_project(&self, id: &str) {
        let _ = self
            .app
            .emit("project-updated", serde_json::json!({ "id": id }));
    }
}

/// Forwards render events to the UI and shows the completion notification.
pub struct TauriEvents {
    pub app: AppHandle,
    pub data: DataDir,
}

impl Events for TauriEvents {
    fn progress(&self, p: &RenderProgress) {
        let _ = self.app.emit("render-progress", p);
    }

    fn project_changed(&self, id: &str) {
        let _ = self
            .app
            .emit("project-updated", serde_json::json!({ "id": id }));
    }

    fn queue_changed(&self, q: &QueueState) {
        let _ = self.app.emit("queue-updated", q);
    }

    fn finished(&self, _id: &str, title: &str, result: &Result<FinishedInfo, String>) {
        let notify = Settings::load(&self.data)
            .map(|s| s.notify_on_finish)
            .unwrap_or(true);
        if !notify {
            return;
        }
        let n = self.app.notification().builder();
        let _ = match result {
            Ok(info) => {
                let mins = info.duration_secs / 60;
                n.title("Your audiobook is ready")
                    .body(format!("“{title}” — {} h {} min", mins / 60, mins % 60))
                    .show()
            }
            Err(msg) => n
                .title(format!("Couldn't finish “{title}”"))
                .body(msg.clone())
                .show(),
        };
    }

    fn notice(&self, code: &str) {
        if code == "gpu-fallback"
            && let Ok(mut s) = Settings::load(&self.data)
        {
            s.gpu = false;
            let _ = s.save(&self.data);
        }
        let _ = self.app.emit("notice", serde_json::json!({ "code": code }));
    }
}
