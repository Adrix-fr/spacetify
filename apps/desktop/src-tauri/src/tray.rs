//! The icon in the notification area: open, apply, restore, quit. Closing
//! the window only hides it; the background service is a separate program
//! and keeps running either way.

use spacetify_core::{Lang, tr};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

const TRAY_ID: &str = "main";

pub fn create(app: &AppHandle, lang: Lang) -> tauri::Result<()> {
    let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("spacetify")
        .menu(&menu(app, lang)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            action @ ("apply" | "restore") => {
                show_main(app);
                // The window runs the same steps as its own buttons, and asks
                // before closing Spotify.
                let _ = app.emit("tray-action", action);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn menu<R: Runtime>(app: &AppHandle<R>, lang: Lang) -> tauri::Result<Menu<R>> {
    let item = |id: &str, text: String| MenuItem::with_id(app, id, text, true, None::<&str>);
    Menu::with_items(
        app,
        &[
            &item("open", tr!(lang, "Open spacetify", "Ouvrir spacetify"))?,
            &PredefinedMenuItem::separator(app)?,
            &item("apply", tr!(lang, "Apply the mods", "Appliquer les mods"))?,
            &item(
                "restore",
                tr!(
                    lang,
                    "Restore original Spotify",
                    "Restaurer Spotify d'origine"
                ),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit", tr!(lang, "Quit", "Quitter"))?,
        ],
    )
}

/// Rebuilds the menu after a language change.
pub fn refresh(app: &AppHandle, lang: Lang) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), menu(app, lang)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
