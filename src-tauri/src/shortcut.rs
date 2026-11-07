use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState, ShortcutWrapper};

use crate::clipboard::ClipboardError;

const PANEL_LABEL: &str = "clipboard-panel";

pub fn show_panel(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || present(&app, false));
}

pub fn toggle_panel(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || present(&app, true));
}

pub fn parse_shortcut(shortcut: &str) -> Result<(), ClipboardError> {
    ShortcutWrapper::try_from(shortcut).map(|_| ()).map_err(|_| ClipboardError::new("invalid_input"))
}

pub fn sync_shortcut(
    app: &AppHandle,
    desired: &str,
    active: bool,
    registered: &mut Option<String>,
) -> Result<(), ClipboardError> {
    if !active {
        if registered.is_some() {
            let _ = app.global_shortcut().unregister_all();
            *registered = None;
        }
        return Ok(());
    }
    if registered.as_deref() == Some(desired) {
        return Ok(());
    }
    parse_shortcut(desired)?;
    let previous = registered.clone();
    if let Some(previous) = previous.as_deref() {
        let _ = app.global_shortcut().unregister(previous);
    }
    match register(app, desired) {
        Ok(()) => {
            *registered = Some(desired.to_string());
            Ok(())
        }
        Err(error) => {
            eprintln!("clipboard shortcut: {error}");
            *registered = None;
            if let Some(previous) = previous {
                if register(app, &previous).is_ok() {
                    *registered = Some(previous);
                }
            }
            Err(ClipboardError::new("shortcut_conflict"))
        }
    }
}

fn register(app: &AppHandle, shortcut: &str) -> Result<(), tauri_plugin_global_shortcut::Error> {
    let app_handle = app.clone();
    app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            toggle_panel(&app_handle);
        }
    })
}

fn present(app: &AppHandle, toggle: bool) {
    if let Some(window) = app.get_webview_window(PANEL_LABEL) {
        let visible = window.is_visible().unwrap_or(false);
        if toggle && visible {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
        return;
    }
    if let Err(error) = open_panel(app) {
        eprintln!("clipboard panel: {error}");
    }
}

fn open_panel(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, PANEL_LABEL, WebviewUrl::App("index.html".into()))
        .title("Pixo")
        .inner_size(440.0, 560.0)
        .min_inner_size(320.0, 420.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .resizable(true)
        .focused(true)
        .center()
        .build()?;
    Ok(())
}
