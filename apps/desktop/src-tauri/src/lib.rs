//! spacetify desktop app: a window and a tray icon on top of
//! `spacetify_core`, like the CLI. It holds no mod logic of its own.

mod commands;
mod model;
mod state;
mod terminal;
mod tray;

use std::sync::PoisonError;

use tauri::{Manager, RunEvent, WindowEvent};

pub fn run() {
    spacetify_core::logs::init(0);
    let state = state::AppState::load();
    let lang = state.lang();

    let built = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(move |app| {
            tray::create(app.handle(), lang)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps the app in the notification area.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::apply,
            commands::restore,
            commands::launch_spotify,
            commands::list_mods,
            commands::install_mod,
            commands::install_example,
            commands::uninstall_mod,
            commands::set_theme,
            commands::set_extension,
            commands::set_setting,
            commands::doctor,
            commands::repair_info,
            commands::set_service_running,
            commands::set_autostart,
            commands::service_check,
            commands::get_settings,
            commands::set_language,
            terminal::run_cli,
            terminal::stop_cli,
        ])
        .build(tauri::generate_context!());

    match built {
        Ok(app) => app.run(|app, event| {
            // A command started from the Terminal tab ends with the app.
            if let RunEvent::Exit = event
                && let Some(state) = app.try_state::<state::Shared>()
                && let Some(child) = state
                    .terminal
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .as_mut()
            {
                let _ = child.kill();
            }
        }),
        Err(err) => {
            tracing::error!("could not start the window: {err}");
            std::process::exit(1);
        }
    }
}
