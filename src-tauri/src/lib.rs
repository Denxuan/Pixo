mod clipboard;
mod panel_anchor;
mod shortcut;
mod tray;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|arg| arg == "--clipboard-panel") {
                shortcut::show_panel(app);
            } else if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = clipboard::initialize(app.handle().clone()).map_err(|error| {
                eprintln!("clipboard init: {error}");
                Box::new(error) as Box<dyn std::error::Error>
            })?;
            app.manage(state);
            #[cfg(target_os = "macos")]
            if let Err(error) = shortcut::prepare_panel(app.handle()) {
                eprintln!("clipboard panel: {error}");
            }
            ensure_main_window(app.handle())?;
            if let Err(error) = tray::install(app.handle()) {
                eprintln!("clipboard tray: {error}");
            }
            if std::env::args().any(|arg| arg == "--clipboard-panel") {
                shortcut::show_panel(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            clipboard::commands::clipboard_list,
            clipboard::commands::clipboard_get,
            clipboard::commands::clipboard_image,
            clipboard::commands::clipboard_source_icon,
            clipboard::commands::clipboard_copy,
            clipboard::commands::clipboard_set_pinned,
            clipboard::commands::clipboard_delete,
            clipboard::commands::clipboard_clear,
            clipboard::commands::clipboard_read_now,
            clipboard::commands::clipboard_status,
            clipboard::commands::clipboard_set_paused,
            clipboard::commands::clipboard_get_settings,
            clipboard::commands::clipboard_update_settings,
            shortcut::clipboard_dismiss_panel,
            tray::clipboard_set_tray_labels,
            tray::clipboard_take_main_request,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub(crate) fn ensure_main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("main").is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("pixo")
        .inner_size(1100.0, 720.0)
        .min_inner_size(860.0, 600.0)
        .build()?;
    Ok(())
}
