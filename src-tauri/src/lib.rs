mod commands;
mod error;
mod player;
mod state;

use aloud_core::{
    DataDir, fsx, project::ProjectStore, queue::QueueState, render::RenderService,
    settings::Settings,
};
use std::sync::{Arc, Mutex, atomic::AtomicBool};
use tauri::{Emitter, Manager, WindowEvent};

use state::{AppState, TauriEvents};

/// Points espeak-ng at its bundled data before any phonemization happens.
fn configure_espeak(app: &tauri::App) {
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .filter(|d| d.join("espeak-ng-data").join("en_dict").exists());
    let dev = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    let dir = bundled.or_else(|| {
        dev.join("espeak-ng-data")
            .join("en_dict")
            .exists()
            .then_some(dev)
    });
    if let Some(dir) = dir {
        // SAFETY: runs in `setup`, before any phonemizer call and before the render scheduler
        // and player threads exist, so no other thread is reading the environment.
        unsafe { std::env::set_var("PIPER_ESPEAKNG_DATA_DIRECTORY", dir) };
    }
}

fn render_busy(app: &tauri::AppHandle) -> bool {
    app.try_state::<Arc<AppState>>()
        .is_some_and(|s| s.render.is_busy())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            configure_espeak(app);
            let data = match DataDir::from_env() {
                Some(d) => d,
                None => DataDir::new(app.path().app_data_dir()?),
            };
            std::fs::create_dir_all(data.root())?;
            // A corrupt file is kept as `.bak` and replaced by defaults, once, so the app starts.
            let mut startup_notices: Vec<&'static str> = Vec::new();
            match fsx::set_aside_if_corrupt::<Settings>(&data.settings()) {
                Ok(true) => startup_notices.push("settings-reset"),
                Ok(false) => {}
                Err(e) => log::warn!("couldn't set aside settings.json: {e}"),
            }
            match fsx::set_aside_if_corrupt::<QueueState>(&data.queue()) {
                Ok(true) => startup_notices.push("queue-reset"),
                Ok(false) => {}
                Err(e) => log::warn!("couldn't set aside queue.json: {e}"),
            }
            let handle = app.handle().clone();
            let render = RenderService::new(
                data.clone(),
                Arc::new(TauriEvents {
                    app: handle.clone(),
                    data: data.clone(),
                }),
            );
            app.manage(Arc::new(AppState {
                store: ProjectStore::new(data.clone()),
                data,
                render,
                player: player::Player::spawn(handle.clone()),
                app: handle,
                settings_lock: Mutex::new(()),
                download_cancel: AtomicBool::new(false),
                downloading: AtomicBool::new(false),
                pending_update: Mutex::new(None),
                startup_notices: Mutex::new(startup_notices),
            }));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let busy = render_busy(window.app_handle());
                if busy {
                    api.prevent_close();
                    let _ = window.app_handle().emit("confirm-quit", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::update_settings,
            commands::download_models,
            commands::cancel_download,
            commands::remove_model,
            commands::list_projects,
            commands::import_epub,
            commands::get_project,
            commands::update_project,
            commands::set_cover,
            commands::delete_project,
            commands::list_voices,
            commands::preview_voice,
            commands::get_tricky_words,
            commands::dismiss_word,
            commands::upsert_pronunciation,
            commands::delete_pronunciation,
            commands::test_pronunciation,
            commands::enqueue,
            commands::cancel_render,
            commands::pause_queue,
            commands::resume_queue,
            commands::move_in_queue,
            commands::player_play_chapter,
            commands::player_toggle,
            commands::player_stop,
            commands::player_seek,
            commands::check_for_update,
            commands::skip_update,
            commands::install_update,
            commands::project_cache_size,
            commands::clear_project_cache,
            commands::storage_size,
            commands::clear_previews,
            commands::open_data_dir,
            commands::open_about,
            commands::open_output,
            commands::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Aloud")
        .run(|app, event| {
            // `code: None` is the user asking to quit (Cmd+Q, Dock → Quit); `quit_app` exits with
            // `Some(0)` once the job has stopped, so it is never blocked here.
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = &event
                && render_busy(app)
            {
                api.prevent_exit();
                let _ = app.emit("confirm-quit", ());
            }
        });
}
