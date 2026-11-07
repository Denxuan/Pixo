use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use super::{MonitorEvent, Platform};
use crate::clipboard::error::ClipboardError;
use crate::clipboard::model::{Capture, PlatformCaps};

enum Session {
    Wayland,
    X11,
    None,
}

fn session() -> Session {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        Session::Wayland
    } else if std::env::var_os("DISPLAY").is_some() {
        Session::X11
    } else {
        Session::None
    }
}

enum Backend {
    Wayland(super::wayland::WaylandBackend),
    X11(super::x11::X11Backend),
    Missing { platform: &'static str, reason: String },
}

pub struct LinuxPlatform {
    backend: Backend,
}

impl LinuxPlatform {
    pub fn start(events: Sender<MonitorEvent>, recording: Arc<AtomicBool>) -> Result<Self, ClipboardError> {
        let backend = match session() {
            Session::Wayland => match super::wayland::WaylandBackend::start(events, recording) {
                Ok(backend) => Backend::Wayland(backend),
                Err(reason) => Backend::Missing {
                    platform: "linux-wayland",
                    reason,
                },
            },
            Session::X11 => match super::x11::X11Backend::start(events, recording) {
                Ok(backend) => Backend::X11(backend),
                Err(reason) => Backend::Missing {
                    platform: "linux-x11",
                    reason,
                },
            },
            Session::None => Backend::Missing {
                platform: "linux",
                reason: "no_display".into(),
            },
        };
        Ok(Self { backend })
    }
}

impl Platform for LinuxPlatform {
    fn capabilities(&self) -> PlatformCaps {
        match &self.backend {
            Backend::Wayland(backend) => backend.capabilities(),
            Backend::X11(backend) => backend.capabilities(),
            Backend::Missing { platform, reason } => PlatformCaps {
                platform,
                can_monitor: false,
                can_read_source_app: false,
                can_global_shortcut: false,
                reason: Some(reason.clone()),
            },
        }
    }

    fn set_recording(&self, recording: bool) {
        match &self.backend {
            Backend::Wayland(backend) => backend.set_recording(recording),
            Backend::X11(backend) => backend.set_recording(recording),
            Backend::Missing { .. } => {}
        }
    }

    fn set_kinds(&self, text: bool, image: bool, files: bool) {
        match &self.backend {
            Backend::Wayland(backend) => backend.set_kinds(text, image, files),
            Backend::X11(backend) => backend.set_kinds(text, image, files),
            Backend::Missing { .. } => {}
        }
    }

    fn read_now(&self) -> Result<Capture, ClipboardError> {
        match &self.backend {
            Backend::Wayland(backend) => backend.read_now(),
            Backend::X11(backend) => backend.read_now(),
            Backend::Missing { reason, .. } => Err(ClipboardError::with_params(
                "unsupported",
                serde_json::json!({ "reason": reason }),
            )),
        }
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        match &self.backend {
            Backend::Wayland(backend) => backend.write_text(text),
            Backend::X11(backend) => backend.write_text(text),
            Backend::Missing { .. } => Err(ClipboardError::new("unsupported")),
        }
    }

    fn write_image(&self, width: u32, height: u32, rgba: &[u8]) -> Result<(), ClipboardError> {
        match &self.backend {
            Backend::Wayland(backend) => backend.write_image(width, height, rgba),
            Backend::X11(backend) => backend.write_image(width, height, rgba),
            Backend::Missing { .. } => Err(ClipboardError::new("unsupported")),
        }
    }

    fn write_files(&self, paths: &[String]) -> Result<(), ClipboardError> {
        match &self.backend {
            Backend::Wayland(backend) => backend.write_files(paths),
            Backend::X11(backend) => backend.write_files(paths),
            Backend::Missing { .. } => Err(ClipboardError::new("unsupported")),
        }
    }
}
