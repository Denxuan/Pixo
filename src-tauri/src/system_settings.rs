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

const LATEST_RELEASE: &str = "https://api.github.com/repos/Denxuan/Pixo/releases/latest";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub latest: String,
    pub url: String,
    pub update_available: bool,
}

#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

#[tauri::command]
pub fn system_check_update(app: AppHandle) -> Result<UpdateStatus, String> {
    let current = app.package_info().version.to_string();
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .get(LATEST_RELEASE)
        .set("User-Agent", "Pixo")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|_| "unavailable".to_string())?;
    let body = response
        .into_string()
        .map_err(|_| "unavailable".to_string())?;
    let release: GitHubRelease =
        serde_json::from_str(&body).map_err(|_| "unavailable".to_string())?;
    update_from_release(&current, &release.tag_name, &release.html_url).map_err(str::to_string)
}

fn update_from_release(
    current: &str,
    tag_name: &str,
    html_url: &str,
) -> Result<UpdateStatus, &'static str> {
    let latest = tag_name.strip_prefix('v').unwrap_or(tag_name);
    if !is_stable(latest) || !html_url.starts_with("https://github.com/Denxuan/Pixo/releases/") {
        return Err("unavailable");
    }
    Ok(UpdateStatus {
        update_available: is_newer(current, latest),
        latest: latest.to_string(),
        url: html_url.to_string(),
    })
}

fn is_stable(version: &str) -> bool {
    version_parts(version).is_some()
}

fn is_newer(current: &str, latest: &str) -> bool {
    match (version_parts(current), version_parts(latest)) {
        (Some(current), Some(latest)) => latest > current,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn version_parts(version: &str) -> Option<[u64; 3]> {
    let mut numbers = [0; 3];
    let mut parts = version.split('.');
    for number in &mut numbers {
        let text = parts.next()?;
        if text.len() > 1 && text.starts_with('0') {
            return None;
        }
        *number = text.parse().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(numbers)
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

    #[test]
    fn newer_github_release_is_detected_numerically() {
        let status = super::update_from_release(
            "0.1.6",
            "v0.1.10",
            "https://github.com/Denxuan/Pixo/releases/tag/v0.1.10",
        )
        .unwrap();
        assert!(status.update_available);
        assert_eq!(status.latest, "0.1.10");
        let current = super::update_from_release(
            "0.1.10",
            "v0.1.9",
            "https://github.com/Denxuan/Pixo/releases/tag/v0.1.9",
        )
        .unwrap();
        assert!(!current.update_available);
        assert!(
            super::update_from_release(
                "0.1.6",
                "v0.1.7-beta",
                "https://github.com/Denxuan/Pixo/releases/tag/v0.1.7-beta"
            )
            .is_err()
        );
        assert!(
            super::update_from_release(
                "0.1.6",
                "v0.1.7",
                "https://example.com/releases/tag/v0.1.7"
            )
            .is_err()
        );
    }
}
