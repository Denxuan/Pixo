use serde::Serialize;

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
use dispatch2::DispatchQueue;
#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSPasteboard, NSPasteboardAccessBehavior, NSPasteboardTypeString};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSDictionary, NSNumber, NSString};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemPermission {
    pub id: String,
    pub granted: bool,
}

pub fn status() -> Vec<SystemPermission> {
    #[cfg(target_os = "macos")]
    {
        on_main(snapshot)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

pub fn prepare() -> Vec<SystemPermission> {
    #[cfg(target_os = "macos")]
    {
        let prompt = !PREPARED.swap(true, Ordering::AcqRel);
        on_main(move || {
            if prompt {
                prompt_all();
            }
            snapshot()
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

pub fn open(id: &str) -> Result<(), ()> {
    let Some(url) = settings_url(id) else {
        return Err(());
    };
    open_settings_url(&url)
}

#[cfg(target_os = "macos")]
pub(crate) fn prompt_accessibility() {
    static ASKED: AtomicBool = AtomicBool::new(false);
    if ASKED.swap(true, Ordering::Relaxed) {
        return;
    }
    // The shortcut callback cannot re-enter AppKit. Ask on the next turn.
    DispatchQueue::main().exec_async(|| {
        let key = NSString::from_str("AXTrustedCheckOptionPrompt");
        let yes = NSNumber::numberWithBool(true);
        let options = NSDictionary::from_slices(&[&*key], &[&*yes]);
        let _ = unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&options).cast()) };
    });
}

#[cfg(target_os = "macos")]
static PREPARED: AtomicBool = AtomicBool::new(false);

fn required_permissions(
    accessibility: bool,
    input_monitoring: bool,
    pasteboard: Option<&str>,
) -> Vec<SystemPermission> {
    vec![
        SystemPermission {
            id: "accessibility".to_string(),
            granted: accessibility,
        },
        SystemPermission {
            id: "inputMonitoring".to_string(),
            granted: input_monitoring,
        },
        SystemPermission {
            id: "pasteboard".to_string(),
            granted: pasteboard_granted(pasteboard),
        },
    ]
}

fn pasteboard_granted(behavior: Option<&str>) -> bool {
    behavior == Some("allow")
}

fn settings_url(id: &str) -> Option<String> {
    let anchor = match id {
        "accessibility" => "Privacy_Accessibility",
        "inputMonitoring" => "Privacy_ListenEvent",
        "pasteboard" => "Privacy_Pasteboard",
        _ => return None,
    };
    Some(format!(
        "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?{anchor}"
    ))
}

#[cfg(target_os = "macos")]
fn snapshot() -> Vec<SystemPermission> {
    required_permissions(
        accessibility_granted(),
        input_monitoring_granted(),
        pasteboard_behavior().as_deref(),
    )
}

#[cfg(target_os = "macos")]
fn prompt_all() {
    prompt_accessibility();
    if !input_monitoring_granted() {
        let _ = unsafe { CGRequestListenEventAccess() };
    }
    register_pasteboard();
}

#[cfg(target_os = "macos")]
fn accessibility_granted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

#[cfg(target_os = "macos")]
fn input_monitoring_granted() -> bool {
    unsafe { CGPreflightListenEventAccess() }
}

#[cfg(target_os = "macos")]
fn pasteboard_behavior() -> Option<String> {
    let behavior = NSPasteboard::generalPasteboard().accessBehavior();
    Some(
        match behavior {
            NSPasteboardAccessBehavior::AlwaysAllow => "allow",
            NSPasteboardAccessBehavior::AlwaysDeny => "deny",
            NSPasteboardAccessBehavior::Ask => "ask",
            _ => "default",
        }
        .to_string(),
    )
}

#[cfg(target_os = "macos")]
fn register_pasteboard() {
    let behavior = pasteboard_behavior();
    if pasteboard_granted(behavior.as_deref()) || behavior.as_deref() == Some("deny") {
        return;
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    let string_type = unsafe { &*NSPasteboardTypeString };
    let _ = pasteboard.stringForType(string_type);
}

#[cfg(target_os = "macos")]
fn on_main<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    if MainThreadMarker::new().is_some() {
        return work();
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    DispatchQueue::main().exec_sync(move || {
        let _ = sender.send(work());
    });
    receiver
        .recv()
        .expect("main queue returned the permission result")
}

#[cfg(target_os = "macos")]
fn open_settings_url(url: &str) -> Result<(), ()> {
    match std::process::Command::new("open").arg(url).status() {
        Ok(status) if status.success() => Ok(()),
        _ => Err(()),
    }
}

#[cfg(not(target_os = "macos"))]
fn open_settings_url(_url: &str) -> Result<(), ()> {
    Err(())
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> u8;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
}

#[tauri::command]
pub fn permissions_status() -> Vec<SystemPermission> {
    status()
}

#[tauri::command]
pub fn permissions_prepare() -> Vec<SystemPermission> {
    prepare()
}

#[tauri::command]
pub fn permissions_open(id: String) -> Result<(), crate::clipboard::ClipboardError> {
    open(&id).map_err(|_| crate::clipboard::ClipboardError::new("unsupported"))
}

#[cfg(test)]
mod tests {
    use super::{pasteboard_granted, required_permissions, settings_url};

    #[test]
    fn pasteboard_stays_closed_until_always_allow() {
        assert!(pasteboard_granted(Some("allow")));
        assert!(!pasteboard_granted(Some("ask")));
        assert!(!pasteboard_granted(Some("deny")));
        assert!(!pasteboard_granted(Some("default")));
        assert!(!pasteboard_granted(None));
    }

    #[test]
    fn settings_open_the_matching_privacy_pane() {
        assert_eq!(
            settings_url("accessibility").as_deref(),
            Some(
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility"
            )
        );
        assert_eq!(
            settings_url("inputMonitoring").as_deref(),
            Some(
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ListenEvent"
            )
        );
        assert_eq!(
            settings_url("pasteboard").as_deref(),
            Some(
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Pasteboard"
            )
        );
        assert!(settings_url("screen").is_none());
    }

    #[test]
    fn required_permissions_keep_stable_ids() {
        let items = required_permissions(true, false, Some("ask"));
        assert_eq!(
            items
                .iter()
                .map(|item| (item.id.as_str(), item.granted))
                .collect::<Vec<_>>(),
            vec![
                ("accessibility", true),
                ("inputMonitoring", false),
                ("pasteboard", false),
            ]
        );
    }
}
