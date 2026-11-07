use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState, ShortcutWrapper};

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSPoint, NSRect, NSSize};

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
    ShortcutWrapper::try_from(shortcut)
        .map(|_| ())
        .map_err(|_| ClipboardError::new("invalid_input"))
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
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
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
            reveal(&window);
        }
        return;
    }
    if let Err(error) = open_panel(app) {
        eprintln!("clipboard panel: {error}");
    }
}

fn open_panel(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, PANEL_LABEL, WebviewUrl::App("index.html".into()))
        .title("Pixo")
        .inner_size(440.0, 560.0)
        .min_inner_size(320.0, 420.0)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .always_on_top(cfg!(not(target_os = "macos")))
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .resizable(true)
        .visible(false)
        .focused(false)
        .center()
        .build()?;
    reveal(&window);
    Ok(())
}

fn reveal(window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    if place_over_input(window) {
        return;
    }
    #[cfg(not(target_os = "macos"))]
    place_near_cursor(window);
    let _ = window.show();
    let _ = window.set_focus();
}

#[cfg(target_os = "macos")]
fn place_over_input(window: &WebviewWindow) -> bool {
    let Ok(raw) = window.ns_window() else {
        return false;
    };
    let Some(ns_window) = (unsafe { Retained::retain(raw.cast::<NSWindow>()) }) else {
        return false;
    };
    let frame = ns_window.frame();
    let width = if frame.size.width < 50.0 {
        440.0
    } else {
        frame.size.width
    };
    let height = if frame.size.height < 50.0 {
        560.0
    } else {
        frame.size.height
    };
    let anchor = crate::panel_anchor::focused_input();
    let screen = crate::panel_anchor::screen_containing(anchor);
    let (x, y) = crate::panel_anchor::place_beside(anchor, width, height, screen);
    // show() and set_focus() activate the whole app and leave the full-screen space.
    // Order only this panel, above the app that currently owns the display.
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    ns_window.setLevel(NSScreenSaverWindowLevel);
    ns_window.setHidesOnDeactivate(false);
    ns_window.setFrame_display(
        NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)),
        true,
    );
    ns_window.orderFrontRegardless();
    ns_window.setLevel(NSScreenSaverWindowLevel);
    true
}

#[cfg(not(target_os = "macos"))]
fn place_near_cursor(window: &WebviewWindow) {
    let Ok(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let mut x = cursor.x + 16.0;
    let mut y = cursor.y + 16.0;
    let Ok(monitors) = window.available_monitors() else {
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        return;
    };
    if let Some(monitor) = monitors.iter().find(|monitor| {
        let pos = monitor.position();
        let area = monitor.size();
        let left = f64::from(pos.x);
        let top = f64::from(pos.y);
        cursor.x >= left
            && cursor.y >= top
            && cursor.x < left + f64::from(area.width)
            && cursor.y < top + f64::from(area.height)
    }) {
        let pos = monitor.position();
        let area = monitor.size();
        let left = f64::from(pos.x);
        let top = f64::from(pos.y);
        let max_x = left + f64::from(area.width) - f64::from(size.width);
        let max_y = top + f64::from(area.height) - f64::from(size.height);
        x = x.clamp(left, max_x.max(left));
        y = y.clamp(top, max_y.max(top));
    }
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}
