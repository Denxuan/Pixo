use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;

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
    Missing {
        platform: &'static str,
        reason: String,
    },
}

pub struct LinuxPlatform {
    backend: Backend,
}

impl LinuxPlatform {
    pub fn start(
        events: Sender<MonitorEvent>,
        recording: Arc<AtomicBool>,
    ) -> Result<Self, ClipboardError> {
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

    fn write_text(&self, text: &crate::clipboard::model::RichText) -> Result<(), ClipboardError> {
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

    fn write_gif(&self, gif: &[u8], png: &[u8]) -> Result<(), ClipboardError> {
        match &self.backend {
            Backend::Wayland(backend) => backend.write_gif(gif, png),
            Backend::X11(backend) => backend.write_gif(gif, png),
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

pub fn installed_apps() -> Vec<crate::clipboard::model::InstalledApp> {
    catalog()
        .iter()
        .map(|app| crate::clipboard::model::InstalledApp {
            id: app.id.clone(),
            name: app.name.clone(),
            icon_png: app.icon_png.clone(),
        })
        .collect()
}

pub fn source_mark(source: &str) -> Option<crate::clipboard::model::SourceMark> {
    let source = source.trim();
    if source.is_empty() || source.len() > 1024 || source.chars().any(char::is_control) {
        return None;
    }
    let apps = catalog();
    if let Some(app) = apps.iter().find(|app| app.id == source) {
        return Some(crate::clipboard::model::SourceMark {
            name: app.name.clone(),
            icon_png: app.icon_png.clone(),
        });
    }
    let file_name = std::path::Path::new(source)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source);
    let matches: Vec<_> = apps
        .iter()
        .filter(|app| {
            std::path::Path::new(&app.id)
                .file_name()
                .and_then(|name| name.to_str())
                == Some(file_name)
        })
        .collect();
    if matches.len() == 1 {
        return Some(crate::clipboard::model::SourceMark {
            name: matches[0].name.clone(),
            icon_png: matches[0].icon_png.clone(),
        });
    }
    let name = std::path::Path::new(source)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name)
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some(crate::clipboard::model::SourceMark {
            name,
            icon_png: Vec::new(),
        })
    }
}

struct ListedApp {
    id: String,
    name: String,
    icon_png: Vec<u8>,
}

fn catalog() -> &'static Vec<ListedApp> {
    static CATALOG: std::sync::OnceLock<Vec<ListedApp>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(load_catalog)
}

fn load_catalog() -> Vec<ListedApp> {
    let locale = std::env::var("LC_MESSAGES")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    let mut found = Vec::new();
    for root in desktop_roots() {
        collect_desktops(&root, &locale, &mut found);
        if found.len() >= 2000 {
            break;
        }
    }
    let mut apps = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (name, program, icon) in found {
        let Some(id) = resolve_program(&program) else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        apps.push(ListedApp {
            id,
            name,
            icon_png: icon_png(&icon),
        });
        if apps.len() >= 2000 {
            break;
        }
    }
    apps.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then(left.id.cmp(&right.id))
    });
    apps
}

fn desktop_roots() -> Vec<std::path::PathBuf> {
    let mut roots = vec![
        std::path::PathBuf::from("/usr/share/applications"),
        std::path::PathBuf::from("/usr/local/share/applications"),
        std::path::PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        roots.push(home.join(".local/share/applications"));
        roots.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    if let Some(dirs) = std::env::var_os("XDG_DATA_DIRS") {
        for dir in std::env::split_paths(&dirs) {
            roots.push(dir.join("applications"));
        }
    }
    roots
}

fn collect_desktops(dir: &std::path::Path, locale: &str, found: &mut Vec<(String, String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if found.len() >= 2000 {
            return;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(entry) = crate::clipboard::desktop::parse_desktop(&text, locale) else {
            continue;
        };
        found.push((entry.name, entry.exec, entry.icon));
    }
}

fn resolve_program(program: &str) -> Option<String> {
    let path = std::path::Path::new(program);
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        let path_env = std::env::var_os("PATH")?;
        std::env::split_paths(&path_env)
            .map(|dir| dir.join(program))
            .find(|path| path.is_file())?
    };
    if !candidate.is_file() {
        return None;
    }
    std::fs::canonicalize(&candidate)
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

fn icon_png(icon: &str) -> Vec<u8> {
    let icon = icon.trim();
    if icon.is_empty() {
        return Vec::new();
    }
    if let Some(bytes) = read_png(std::path::Path::new(icon)) {
        return bytes;
    }
    let mut roots = vec![
        std::path::PathBuf::from("/usr/share/icons/hicolor/48x48/apps"),
        std::path::PathBuf::from("/usr/share/icons/hicolor/32x32/apps"),
        std::path::PathBuf::from("/usr/share/icons/hicolor/64x64/apps"),
        std::path::PathBuf::from("/usr/share/pixmaps"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        roots.push(home.join(".local/share/icons/hicolor/48x48/apps"));
        roots.push(home.join(".local/share/icons"));
    }
    for root in roots {
        if let Some(bytes) = read_png(&root.join(format!("{icon}.png"))) {
            return bytes;
        }
    }
    Vec::new()
}

fn read_png(path: &std::path::Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > 256 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    Some(bytes)
}
