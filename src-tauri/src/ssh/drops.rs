use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use tauri::{AppHandle, DragDropEvent, Emitter, Manager, WebviewWindow, WindowEvent};

use super::grant;

static INSTALLED: AtomicBool = AtomicBool::new(false);

pub fn install(window: &WebviewWindow) {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = window.app_handle().clone();
    // The main webview is window content, so wry delivers file drops as
    // WindowEvent::DragDrop. WebviewEvent::DragDrop is only for child webviews.
    window.on_window_event(move |event| {
        let WindowEvent::DragDrop(drag) = event else {
            return;
        };
        match drag {
            DragDropEvent::Enter { position, .. } | DragDropEvent::Over { position } => {
                emit_drag(&app, "over", Some((position.x, position.y)));
            }
            DragDropEvent::Leave => emit_drag(&app, "leave", None),
            DragDropEvent::Drop { paths, position } => match grant::issue(paths) {
                Ok(pick) => {
                    let _ = app.emit(
                        "ssh://dropped",
                        json!({
                            "token": pick.token,
                            "names": pick.names,
                            "x": position.x,
                            "y": position.y,
                        }),
                    );
                }
                Err(_) => {}
            },
            _ => {}
        }
    });
}

fn emit_drag(app: &AppHandle, phase: &str, position: Option<(f64, f64)>) {
    let _ = app.emit(
        "ssh://drag",
        match position {
            Some((x, y)) => json!({"phase": phase, "x": x, "y": y}),
            None => json!({"phase": phase}),
        },
    );
}
