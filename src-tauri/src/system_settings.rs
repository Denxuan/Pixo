use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

#[derive(Default)]
pub struct BackgroundMode(pub AtomicBool);

#[tauri::command]
pub fn system_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn system_set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn system_set_background(
    app: AppHandle,
    state: State<'_, BackgroundMode>,
    enabled: bool,
) -> Result<(), String> {
    if enabled && app.tray_by_id("clipboard-status").is_none() {
        return Err("tray_unavailable".into());
    }
    #[cfg(target_os = "macos")]
    app.set_activation_policy(if enabled {
        tauri::ActivationPolicy::Accessory
    } else {
        tauri::ActivationPolicy::Regular
    })
    .map_err(|e| e.to_string())?;
    state.0.store(enabled, Ordering::SeqCst);
    Ok(())
}

pub fn hide_on_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        if should_hide(
            window.label(),
            window.state::<BackgroundMode>().0.load(Ordering::SeqCst),
            window.app_handle().tray_by_id("clipboard-status").is_some(),
        ) {
            if window.hide().is_ok() {
                api.prevent_close();
            }
        }
    }
}

fn should_hide(label: &str, enabled: bool, tray_available: bool) -> bool {
    label == "main" && enabled && tray_available
}

#[cfg(test)]
mod tests {
    use super::should_hide;
    #[test]
    fn only_hide_the_main_window_when_background_and_tray_are_available() {
        assert!(should_hide("main", true, true));
        assert!(!should_hide("main", false, true));
        assert!(!should_hide("main", true, false));
        assert!(!should_hide("clipboard-panel", true, true));
    }
}
