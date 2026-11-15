use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use super::error::ScreenshotError;
use super::session::{capture_from_hotkey, record_from_hotkey};

#[derive(Clone, Copy)]
pub enum HotkeyKind {
    Capture,
    Record,
}

pub fn sync(
    app: &AppHandle,
    desired: &str,
    active: bool,
    registered: &mut Option<String>,
    kind: HotkeyKind,
) -> Result<(), ScreenshotError> {
    if !active {
        if let Some(current) = registered.take() {
            let _ = app.global_shortcut().unregister(current.as_str());
        }
        return Ok(());
    }
    if registered.as_deref() == Some(desired) {
        return Ok(());
    }
    crate::shortcut::parse_shortcut(desired).map_err(|_| ScreenshotError::new("invalid_input"))?;
    let previous = registered.clone();
    if let Some(previous) = previous.as_deref() {
        let _ = app.global_shortcut().unregister(previous);
    }
    let app_handle = app.clone();
    match app
        .global_shortcut()
        .on_shortcut(desired, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                match kind {
                    HotkeyKind::Capture => capture_from_hotkey(&app_handle),
                    HotkeyKind::Record => record_from_hotkey(&app_handle),
                }
            }
        }) {
        Ok(()) => {
            *registered = Some(desired.to_string());
            Ok(())
        }
        Err(error) => {
            eprintln!("screenshot shortcut: {error}");
            *registered = None;
            if let Some(previous) = previous {
                let restore = app.clone();
                if app
                    .global_shortcut()
                    .on_shortcut(previous.as_str(), move |_app, _shortcut, event| {
                        if event.state == ShortcutState::Pressed {
                            match kind {
                                HotkeyKind::Capture => capture_from_hotkey(&restore),
                                HotkeyKind::Record => record_from_hotkey(&restore),
                            }
                        }
                    })
                    .is_ok()
                {
                    *registered = Some(previous);
                }
            }
            Err(ScreenshotError::new("shortcut_conflict"))
        }
    }
}
