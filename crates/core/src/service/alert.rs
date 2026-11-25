//! Desktop notifications. Best effort: a notification that cannot be shown
//! is logged and forgotten, it never stops the service.

/// Shows a short notification with a title and one line of text.
pub fn show(title: &str, body: &str) {
    if let Err(err) = show_native(title, body) {
        tracing::warn!("could not show a notification: {err}");
    }
}

/// Same, with one button. `on_click` runs (on another thread) when the
/// button is pressed, as long as this process runs. Only Windows shows the
/// button; elsewhere this is a plain notification.
pub fn show_with_button(
    title: &str,
    body: &str,
    button: &str,
    on_click: impl Fn() + Send + 'static,
) {
    #[cfg(windows)]
    {
        use tauri_winrt_notification::Toast;
        const ACTION: &str = "spacetify-button";
        let shown = Toast::new(Toast::POWERSHELL_APP_ID)
            .title(title)
            .text1(body)
            .add_button(button, ACTION)
            .on_activated(move |action| {
                // A click on the text itself does nothing: only the button acts.
                if action.as_deref() == Some(ACTION) {
                    on_click();
                }
                Ok(())
            })
            .show();
        if let Err(err) = shown {
            tracing::warn!("could not show a notification: {err}");
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (button, on_click);
        show(title, body);
    }
}

#[cfg(windows)]
fn show_native(title: &str, body: &str) -> Result<(), String> {
    use tauri_winrt_notification::Toast;
    // A program needs a registered id to show notifications. Until the
    // installer of the desktop app registers spacetify's own, PowerShell's
    // id is used: it exists on every Windows, so the notification shows.
    Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body)
        .show()
        .map_err(|err| err.to_string())
}

#[cfg(target_os = "macos")]
fn show_native(title: &str, body: &str) -> Result<(), String> {
    let quote = |text: &str| format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""));
    let script = format!(
        "display notification {} with title {}",
        quote(body),
        quote(title)
    );
    run(std::process::Command::new("osascript").args(["-e", &script]))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn show_native(title: &str, body: &str) -> Result<(), String> {
    run(std::process::Command::new("notify-send").args(["--app-name=spacetify", title, body]))
}

#[cfg(not(any(windows, unix)))]
fn show_native(_title: &str, _body: &str) -> Result<(), String> {
    Err("not supported on this system".into())
}

#[cfg(unix)]
fn run(command: &mut std::process::Command) -> Result<(), String> {
    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(status.to_string()),
        Err(err) => Err(err.to_string()),
    }
}
