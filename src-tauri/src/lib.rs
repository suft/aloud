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

const ESPEAK_DATA_VAR: &str = "PIPER_ESPEAKNG_DATA_DIRECTORY";

fn has_espeak_data(dir: &std::path::Path) -> bool {
    dir.join("espeak-ng-data").join("en_dict").exists()
}

/// Points espeak-ng at its bundled data; returns whether data was found. Must run first thing in
/// `run()`, before the Tauri builder and any thread exist.
fn configure_espeak(context: &tauri::Context) -> bool {
    // `mise.toml` sets the variable for development.
    if std::env::var_os(ESPEAK_DATA_VAR).is_some_and(|v| !v.is_empty()) {
        return true;
    }
    let bundled =
        tauri::utils::platform::resource_dir(context.package_info(), &tauri::Env::default())
            .ok()
            .filter(|d| has_espeak_data(d));
    // The build directory is only a fallback for development builds.
    #[cfg(debug_assertions)]
    let bundled = bundled.or_else(|| {
        let dev = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        has_espeak_data(&dev).then_some(dev)
    });
    let Some(dir) = bundled else { return false };
    // SAFETY: called first thing in `run()`, from main, before the Tauri builder, its plugins or
    // any thread of ours exist, so nothing else reads the environment.
    unsafe { std::env::set_var(ESPEAK_DATA_VAR, dir) };
    true
}

fn render_busy(app: &tauri::AppHandle) -> bool {
    app.try_state::<Arc<AppState>>()
        .is_some_and(|s| s.render.is_busy())
}

pub fn run() {
    let context = tauri::generate_context!();
    let espeak_found = configure_espeak(&context);
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
        .setup(move |app| {
            if !espeak_found {
                log::warn!(
                    "espeak-ng data wasn't found next to Aloud; making audiobooks will fail"
                );
            }
            let data = match DataDir::from_env() {
                Some(d) => d,
                None => DataDir::new(app.path().app_data_dir()?),
            };
            std::fs::create_dir_all(data.root())?;
            if let Ok(rd) = std::fs::read_dir(data.projects()) {
                for entry in rd.flatten() {
                    if let Some(id) = entry.file_name().to_str()
                        && aloud_core::project::is_project_id(id)
                    {
                        state::allow_covers(app.handle(), &data, id);
                    }
                }
            }
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
        .build(context)
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
