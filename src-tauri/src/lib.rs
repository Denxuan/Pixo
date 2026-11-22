mod clipboard;
mod panel_anchor;
mod permissions;
mod screenshot;
mod shortcut;
mod ssh;
mod system_settings;
mod tray;

use tauri::window::Color;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|arg| arg == "--clipboard-panel") {
                shortcut::show_panel(app);
            } else if args.iter().any(|arg| arg == "--screenshot") {
                screenshot::commands::screenshot_open_capture(app.clone());
            } else if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(system_settings::BackgroundMode::default())
        .on_window_event(system_settings::hide_on_close)
        .setup(|app| {
            let state = clipboard::initialize(app.handle().clone()).map_err(|error| {
                eprintln!("clipboard init: {error}");
                Box::new(error) as Box<dyn std::error::Error>
            })?;
            app.manage(state);
            if let Ok(screenshot) = screenshot::initialize(app.handle().clone()) {
                app.manage(screenshot);
            } else {
                eprintln!("screenshot init failed");
            }
            if let Ok(ssh) = ssh::initialize(app.handle().clone()) {
                app.manage(ssh);
            } else {
                eprintln!("ssh init failed");
            }
            #[cfg(target_os = "macos")]
            if let Err(error) = shortcut::prepare_panel(app.handle()) {
                eprintln!("clipboard panel: {error}");
            }
            if let Err(error) = tray::install(app.handle()) {
                eprintln!("clipboard tray: {error}");
            }
            ensure_main_window(app.handle())?;
            if std::env::args().any(|arg| arg == "--clipboard-panel") {
                shortcut::show_panel(app.handle());
                permissions::prepare();
            }
            if std::env::args().any(|arg| arg == "--screenshot") {
                screenshot::commands::screenshot_open_capture(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            clipboard::commands::clipboard_list,
            clipboard::commands::clipboard_get,
            clipboard::commands::clipboard_image,
            clipboard::commands::clipboard_source_icon,
            clipboard::commands::clipboard_installed_apps,
            clipboard::commands::clipboard_copy,
            clipboard::commands::clipboard_ignore_next,
            clipboard::commands::clipboard_set_pinned,
            clipboard::commands::clipboard_delete,
            clipboard::commands::clipboard_clear,
            clipboard::commands::clipboard_read_now,
            clipboard::commands::clipboard_status,
            clipboard::commands::clipboard_set_paused,
            clipboard::commands::clipboard_set_enabled,
            clipboard::commands::clipboard_get_settings,
            clipboard::commands::clipboard_update_settings,
            clipboard::commands::clipboard_set_label,
            clipboard::commands::clipboard_place_text,
            clipboard::commands::clipboard_show_panel,
            shortcut::clipboard_dismiss_panel,
            shortcut::launcher_open,
            tray::clipboard_set_tray_labels,
            tray::clipboard_set_tray_icon,
            tray::clipboard_take_main_request,
            permissions::permissions_status,
            permissions::permissions_prepare,
            permissions::permissions_open,
            system_settings::system_autostart_enabled,
            system_settings::system_set_autostart,
            system_settings::system_set_background,
            system_settings::system_check_update,
            screenshot::commands::screenshot_status,
            screenshot::commands::screenshot_get_settings,
            screenshot::commands::screenshot_update_settings,
            screenshot::commands::screenshot_set_enabled,
            screenshot::commands::screenshot_pick_save_dir,
            screenshot::commands::screenshot_request_access,
            screenshot::commands::screenshot_start,
            screenshot::commands::screenshot_current,
            screenshot::commands::screenshot_frame,
            screenshot::commands::screenshot_overlay_ready,
            screenshot::commands::screenshot_editor_commit,
            screenshot::commands::screenshot_editor_undo,
            screenshot::commands::screenshot_editor_redo,
            screenshot::commands::screenshot_cancel,
            screenshot::commands::screenshot_finish,
            screenshot::commands::screenshot_copy,
            screenshot::commands::screenshot_recognize_text,
            screenshot::commands::screenshot_copy_text,
            screenshot::commands::screenshot_library_list,
            screenshot::commands::screenshot_library_delete,
            screenshot::commands::screenshot_library_copy,
            screenshot::commands::screenshot_library_path,
            screenshot::commands::screenshot_scroll,
            screenshot::commands::screenshot_scroll_stop,
            screenshot::commands::screenshot_scroll_set_auto,
            screenshot::commands::screenshot_scroll_copy,
            screenshot::commands::screenshot_scroll_save,
            screenshot::commands::screenshot_save,
            screenshot::commands::screenshot_adopt_pin,
            screenshot::commands::screenshot_release_pin,
            screenshot::commands::screenshot_pin,
            screenshot::commands::screenshot_pin_ready,
            screenshot::commands::screenshot_pin_image,
            screenshot::commands::screenshot_pin_copy,
            screenshot::commands::screenshot_pin_close,
            screenshot::commands::screenshot_open_capture,
            screenshot::commands::screenshot_record_start,
            screenshot::commands::screenshot_audio_devices,
            screenshot::commands::screenshot_camera_open,
            screenshot::commands::screenshot_camera_close,
            screenshot::commands::screenshot_record_stop,
            screenshot::commands::screenshot_record_preview,
            screenshot::commands::screenshot_record_download,
            screenshot::commands::screenshot_record_copy,
            screenshot::commands::screenshot_record_close,
            ssh::commands::ssh_status,
            ssh::commands::ssh_set_enabled,
            ssh::commands::ssh_set_keepalive,
            ssh::commands::ssh_hosts_list,
            ssh::commands::ssh_host_save,
            ssh::commands::ssh_host_delete,
            ssh::commands::ssh_config_preview,
            ssh::commands::ssh_config_import,
            ssh::commands::ssh_host_forget_key,
            ssh::commands::ssh_pick_key,
            ssh::commands::ssh_connect,
            ssh::commands::ssh_hostkey_decide,
            ssh::commands::ssh_answer,
            ssh::commands::ssh_write,
            ssh::commands::ssh_interrupt,
            ssh::commands::ssh_ack,
            ssh::commands::ssh_resize,
            ssh::commands::ssh_disconnect,
            ssh::commands::ssh_pick_upload,
            ssh::commands::ssh_release_upload,
            ssh::commands::ssh_list_dir,
            ssh::commands::ssh_make_dir,
            ssh::commands::ssh_rename,
            ssh::commands::ssh_remove,
            ssh::commands::ssh_pick_download_dir,
            ssh::commands::ssh_transfer_start,
            ssh::commands::ssh_transfer_decide,
            ssh::commands::ssh_transfer_cancel,
            ssh::commands::ssh_transfer_retry,
            ssh::commands::ssh_transfer_forget,
            ssh::commands::ssh_forward_open,
            ssh::commands::ssh_forward_list,
            ssh::commands::ssh_forward_close,
            ssh::commands::ssh_snippets_list,
            ssh::commands::ssh_snippet_save,
            ssh::commands::ssh_snippet_delete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub(crate) fn ensure_main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window("main") {
        window
    } else {
        WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("pixo")
            .inner_size(1100.0, 720.0)
            .min_inner_size(860.0, 600.0)
            .background_color(window_canvas())
            .build()?
    };
    ssh::install_drop(&window);
    Ok(())
}

fn window_canvas() -> Color {
    if startup_dark() {
        Color(16, 16, 18, 255)
    } else {
        Color(243, 244, 246, 255)
    }
}

#[cfg(target_os = "macos")]
fn startup_dark() -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    let Some(marker) = MainThreadMarker::new() else {
        return false;
    };
    NSApplication::sharedApplication(marker)
        .effectiveAppearance()
        .name()
        .to_string()
        .to_ascii_lowercase()
        .contains("dark")
}

#[cfg(target_os = "windows")]
fn startup_dark() -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    use windows::core::w;

    let mut value = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&mut size),
        )
    };
    status.is_ok() && value == 0
}

#[cfg(target_os = "linux")]
fn startup_dark() -> bool {
    if std::env::var("GTK_THEME")
        .ok()
        .is_some_and(|theme| theme.to_ascii_lowercase().contains("dark"))
    {
        return true;
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        });
    let Some(config) = config else {
        return false;
    };
    for relative in ["gtk-4.0/settings.ini", "gtk-3.0/settings.ini", "kdeglobals"] {
        let Ok(text) = std::fs::read_to_string(config.join(relative)) else {
            continue;
        };
        if config_prefers_dark(&text) {
            return true;
        }
    }
    false
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn startup_dark() -> bool {
    false
}

#[cfg(any(test, target_os = "linux"))]
fn config_prefers_dark(text: &str) -> bool {
    let mut settings = false;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            settings =
                name.eq_ignore_ascii_case("Settings") || name.eq_ignore_ascii_case("General");
            continue;
        }
        if !settings {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key == "gtk-application-prefer-dark-theme"
            && (value == "1" || value.eq_ignore_ascii_case("true"))
        {
            return true;
        }
        if key == "gtk-theme-name" && value.to_ascii_lowercase().contains("dark") {
            return true;
        }
        if key == "ColorScheme" && value.to_ascii_lowercase().contains("dark") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::config_prefers_dark;

    #[test]
    fn gtk_and_kde_dark_settings_are_recognized() {
        assert!(config_prefers_dark(
            "[Settings]\ngtk-application-prefer-dark-theme=1\n"
        ));
        assert!(config_prefers_dark(
            "[Settings]\ngtk-theme-name=Adwaita-dark\n"
        ));
        assert!(config_prefers_dark("[General]\nColorScheme=BreezeDark\n"));
        assert!(!config_prefers_dark(
            "[Settings]\ngtk-application-prefer-dark-theme=0\n"
        ));
        assert!(!config_prefers_dark(
            "dark theme mentioned outside a setting\n"
        ));
    }
}
