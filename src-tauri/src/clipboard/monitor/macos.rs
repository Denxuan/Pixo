use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardAccessBehavior, NSPasteboardTypeFileURL, NSPasteboardTypeString,
    NSPasteboardWriting, NSWorkspace,
};
use objc2_foundation::{NSArray, NSString, NSURL};

use super::{KindGate, MonitorEvent, Platform};
use crate::clipboard::error::ClipboardError;
use crate::clipboard::io;
use crate::clipboard::model::{Capture, Payload, PlatformCaps};
use crate::clipboard::util::{macos_sensitive, path_to_file_uri};

pub struct MacPlatform {
    recording: Arc<AtomicBool>,
    kinds: Arc<KindGate>,
    _thread: JoinHandle<()>,
}

impl MacPlatform {
    pub fn start(events: Sender<MonitorEvent>, recording: Arc<AtomicBool>) -> Self {
        let flag = Arc::clone(&recording);
        let kinds = Arc::new(KindGate::new());
        let kinds_thread = Arc::clone(&kinds);
        let thread = thread::Builder::new()
            .name("pixo-clipboard".into())
            .spawn(move || poll_loop(events, flag, kinds_thread))
            .expect("clipboard monitor thread");
        Self {
            recording,
            kinds,
            _thread: thread,
        }
    }
}

impl Platform for MacPlatform {
    fn capabilities(&self) -> PlatformCaps {
        PlatformCaps {
            platform: "macos",
            can_monitor: true,
            can_read_source_app: true,
            can_global_shortcut: true,
            reason: None,
        }
    }

    fn set_recording(&self, recording: bool) {
        self.recording.store(recording, Ordering::SeqCst);
    }

    fn set_kinds(&self, text: bool, image: bool, files: bool) {
        self.kinds.set(text, image, files);
    }

    fn read_now(&self) -> Result<Capture, ClipboardError> {
        let files = self.kinds.files();
        let image = self.kinds.image();
        let text = self.kinds.text();
        on_main(move || read_pasteboard(files, image, text))
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        let text = text.to_string();
        on_main(move || io::write_text(&text))
    }

    fn write_image(&self, width: u32, height: u32, rgba: &[u8]) -> Result<(), ClipboardError> {
        let rgba = rgba.to_vec();
        on_main(move || io::write_image(width, height, &rgba))
    }

    fn write_files(&self, paths: &[String]) -> Result<(), ClipboardError> {
        let paths = paths.to_vec();
        on_main(move || write_files(&paths))
    }

    fn prepare_file_path(&self, path: &str) -> Option<String> {
        let path = path.to_string();
        on_main(move || filesystem_path(&path))
    }

    fn access(&self) -> Option<String> {
        on_main(access_behavior)
    }
}

fn poll_loop(events: Sender<MonitorEvent>, recording: Arc<AtomicBool>, kinds: Arc<KindGate>) {
    let mut last_change = on_main(change_count);
    loop {
        thread::sleep(Duration::from_millis(500));
        let change = on_main(change_count);
        if change == last_change {
            continue;
        }
        last_change = change;
        if !recording.load(Ordering::SeqCst) {
            continue;
        }
        let files = kinds.files();
        let image = kinds.image();
        let text = kinds.text();
        let event = match on_main(move || read_pasteboard(files, image, text)) {
            Ok(capture) => MonitorEvent::Capture(capture),
            Err(error) => MonitorEvent::Error(error.code),
        };
        if events.send(event).is_err() {
            break;
        }
    }
}

fn on_main<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    if MainThreadMarker::new().is_some() {
        return work();
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    DispatchQueue::main().exec_sync(move || {
        let _ = sender.send(work());
    });
    receiver
        .recv()
        .expect("main queue returned the clipboard result")
}

fn change_count() -> isize {
    NSPasteboard::generalPasteboard().changeCount()
}

fn access_behavior() -> Option<String> {
    let behavior = NSPasteboard::generalPasteboard().accessBehavior();
    Some(
        match behavior {
            NSPasteboardAccessBehavior::AlwaysDeny => "deny",
            NSPasteboardAccessBehavior::AlwaysAllow => "allow",
            NSPasteboardAccessBehavior::Ask => "ask",
            _ => "default",
        }
        .to_string(),
    )
}

fn read_pasteboard(
    files_enabled: bool,
    image_enabled: bool,
    text_enabled: bool,
) -> Result<Capture, ClipboardError> {
    if access_behavior().as_deref() == Some("deny") {
        return Err(ClipboardError::new("permission_denied"));
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    let source_app = frontmost_bundle();
    let types = pasteboard_types(&pasteboard);
    if macos_sensitive(&types) {
        return Ok(Capture {
            payload: None,
            sensitive: true,
            source_app,
        });
    }
    if files_enabled {
        let files = file_paths(&pasteboard);
        if !files.is_empty() {
            return Ok(Capture {
                payload: Some(Payload::Files(files)),
                sensitive: false,
                source_app,
            });
        }
    }
    let payload = io::read_text_or_image_filtered(image_enabled, text_enabled)?;
    Ok(Capture {
        payload,
        sensitive: false,
        source_app,
    })
}

fn pasteboard_types(pasteboard: &NSPasteboard) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(types) = pasteboard.types() {
        for index in 0..types.count() {
            names.push(types.objectAtIndex(index).to_string());
        }
    }
    if let Some(items) = pasteboard.pasteboardItems() {
        for index in 0..items.count() {
            let item_types = items.objectAtIndex(index).types();
            for type_index in 0..item_types.count() {
                names.push(item_types.objectAtIndex(type_index).to_string());
            }
        }
    }
    names
}

fn file_paths(pasteboard: &NSPasteboard) -> Vec<String> {
    let mut paths = Vec::new();
    let Some(items) = pasteboard.pasteboardItems() else {
        return paths;
    };
    let file_type = unsafe { &*NSPasteboardTypeFileURL };
    for index in 0..items.count() {
        let Some(value) = items.objectAtIndex(index).stringForType(file_type) else {
            continue;
        };
        if let Some(path) = filesystem_path(&value.to_string()) {
            paths.push(path);
        }
    }
    paths
}

fn frontmost_bundle() -> Option<String> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let identifier = app.bundleIdentifier()?.to_string();
    if identifier.is_empty() {
        None
    } else {
        Some(identifier)
    }
}

fn filesystem_path(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let reference = if raw.len() >= 5 && raw[..5].eq_ignore_ascii_case("file:") {
        raw.to_string()
    } else if raw.starts_with("/.file/") {
        format!("file://{raw}")
    } else if raw.starts_with('/') {
        return Some(raw.to_string());
    } else {
        return None;
    };
    let url = NSURL::URLWithString(&NSString::from_str(&reference))?;
    let path = url
        .filePathURL()
        .and_then(|resolved| resolved.path())
        .or_else(|| url.path())?;
    let path = path.to_string();
    if path.is_empty() || path.starts_with("/.file/") {
        None
    } else {
        Some(path)
    }
}

fn file_url(path: &str) -> Option<Retained<NSURL>> {
    let path = filesystem_path(path)?;
    NSURL::URLWithString(&NSString::from_str(&path_to_file_uri(&path)))
}

fn write_files(paths: &[String]) -> Result<(), ClipboardError> {
    let urls: Vec<_> = paths.iter().filter_map(|path| file_url(path)).collect();
    if urls.is_empty() {
        return Err(ClipboardError::new("file_missing"));
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let writers: Vec<&ProtocolObject<dyn NSPasteboardWriting>> = urls
        .iter()
        .map(|url| ProtocolObject::from_ref::<NSURL>(&**url))
        .collect();
    let array = NSArray::from_slice(&writers);
    if !pasteboard.writeObjects(&array) {
        return Err(ClipboardError::new("busy"));
    }
    let text = NSString::from_str(&paths.join("\n"));
    let string_type = unsafe { &*NSPasteboardTypeString };
    if !pasteboard.setString_forType(&text, string_type) {
        return Err(ClipboardError::new("busy"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use objc2_foundation::{NSString, NSURL};

    use super::filesystem_path;

    #[test]
    fn resolves_file_urls_without_panicking() {
        assert_eq!(filesystem_path("/Users").as_deref(), Some("/Users"));
        assert_eq!(filesystem_path("file:///Users").as_deref(), Some("/Users"));
        assert_eq!(filesystem_path("file:///.file/id=missing").as_deref(), None);
        let url = NSURL::fileURLWithPath(&NSString::from_str("/Users"));
        let Some(reference) = url.fileReferenceURL() else {
            return;
        };
        let Some(absolute) = reference.absoluteString() else {
            return;
        };
        let resolved = filesystem_path(&absolute.to_string()).expect("file reference resolves");
        assert!(resolved.starts_with('/'));
        assert!(!resolved.starts_with("/.file/"));
    }
}
