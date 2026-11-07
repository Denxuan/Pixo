use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use super::error::ClipboardError;
use super::model::{Capture, PlatformCaps, RichText, SourceMark};

pub enum MonitorEvent {
    Capture(Capture),
    Error(&'static str),
}

pub struct KindGate {
    text: AtomicBool,
    image: AtomicBool,
    files: AtomicBool,
}

impl KindGate {
    pub fn new() -> Self {
        Self {
            text: AtomicBool::new(true),
            image: AtomicBool::new(true),
            files: AtomicBool::new(true),
        }
    }

    pub fn set(&self, text: bool, image: bool, files: bool) {
        self.text.store(text, Ordering::SeqCst);
        self.image.store(image, Ordering::SeqCst);
        self.files.store(files, Ordering::SeqCst);
    }

    pub fn text(&self) -> bool {
        self.text.load(Ordering::SeqCst)
    }

    pub fn image(&self) -> bool {
        self.image.load(Ordering::SeqCst)
    }

    pub fn files(&self) -> bool {
        self.files.load(Ordering::SeqCst)
    }
}

pub trait Platform: Send + Sync {
    fn capabilities(&self) -> PlatformCaps;
    fn set_recording(&self, recording: bool);
    fn set_kinds(&self, text: bool, image: bool, files: bool) {
        let _ = (text, image, files);
    }
    fn read_now(&self) -> Result<Capture, ClipboardError>;
    fn write_text(&self, text: &RichText) -> Result<(), ClipboardError>;
    fn write_image(&self, width: u32, height: u32, rgba: &[u8]) -> Result<(), ClipboardError>;
    fn write_files(&self, paths: &[String]) -> Result<(), ClipboardError>;
    fn prepare_file_path(&self, path: &str) -> Option<String> {
        let path = path.trim();
        if path.is_empty() {
            None
        } else {
            Some(path.to_string())
        }
    }
    fn access(&self) -> Option<String> {
        None
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

pub fn source_mark(source: &str) -> Option<SourceMark> {
    #[cfg(target_os = "macos")]
    {
        macos::source_mark(source)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = source;
        None
    }
}

pub fn start(
    events: Sender<MonitorEvent>,
    recording: Arc<AtomicBool>,
) -> Result<Box<dyn Platform>, ClipboardError> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacPlatform::start(events, recording)))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(windows::WindowsPlatform::start(
            events, recording,
        )?))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(linux::LinuxPlatform::start(events, recording)?))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (events, recording);
        Ok(Box::new(UnsupportedPlatform {
            reason: "platform".to_string(),
        }))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub struct UnsupportedPlatform {
    reason: String,
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
impl Platform for UnsupportedPlatform {
    fn capabilities(&self) -> PlatformCaps {
        PlatformCaps {
            platform: "unknown",
            can_monitor: false,
            can_read_source_app: false,
            can_global_shortcut: false,
            reason: Some(self.reason.clone()),
        }
    }

    fn set_recording(&self, recording: bool) {
        let _ = recording;
    }

    fn read_now(&self) -> Result<Capture, ClipboardError> {
        Err(ClipboardError::with_params(
            "unsupported",
            serde_json::json!({ "reason": self.reason }),
        ))
    }

    fn write_text(&self, _text: &RichText) -> Result<(), ClipboardError> {
        Err(ClipboardError::new("unsupported"))
    }

    fn write_image(&self, _width: u32, _height: u32, _rgba: &[u8]) -> Result<(), ClipboardError> {
        Err(ClipboardError::new("unsupported"))
    }

    fn write_files(&self, _paths: &[String]) -> Result<(), ClipboardError> {
        Err(ClipboardError::new("unsupported"))
    }
}
