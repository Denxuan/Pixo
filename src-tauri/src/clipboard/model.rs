use serde::{Deserialize, Serialize};

use super::error::ClipboardError;

pub const DEFAULT_SHORTCUT: &str = "CommandOrControl+Shift+V";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    Text,
    Image,
    Files,
}

impl ClipKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Files => "files",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "image" => Some(Self::Image),
            "files" => Some(Self::Files),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Payload {
    Text(String),
    Image {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Files(Vec<String>),
}

impl Payload {
    pub fn kind(&self) -> ClipKind {
        match self {
            Self::Text(_) => ClipKind::Text,
            Self::Image { .. } => ClipKind::Image,
            Self::Files(_) => ClipKind::Files,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Capture {
    pub payload: Option<Payload>,
    pub sensitive: bool,
    pub source_app: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMark {
    pub name: String,
    pub icon_png: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: i64,
    pub kind: String,
    pub preview: String,
    pub file_names: Vec<String>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub size_bytes: i64,
    pub source_app: Option<String>,
    pub pinned: bool,
    pub created_at: i64,
    pub last_copied_at: i64,
    pub copy_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub id: i64,
    pub kind: String,
    pub text: Option<String>,
    pub files: Vec<String>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub size_bytes: i64,
    pub source_app: Option<String>,
    pub pinned: bool,
    pub created_at: i64,
    pub last_copied_at: i64,
    pub copy_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPage {
    pub items: Vec<Summary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub enabled: bool,
    pub paused: bool,
    pub record_text: bool,
    pub record_image: bool,
    pub record_files: bool,
    pub max_items: u32,
    pub retention_days: u32,
    pub max_text_bytes: u64,
    pub max_image_bytes: u64,
    pub shortcut: String,
    pub excluded_apps: Vec<String>,
    pub skip_sensitive: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            paused: false,
            record_text: true,
            record_image: true,
            record_files: true,
            max_items: 500,
            retention_days: 30,
            max_text_bytes: 1024 * 1024,
            max_image_bytes: 20 * 1024 * 1024,
            shortcut: DEFAULT_SHORTCUT.to_string(),
            excluded_apps: Vec::new(),
            skip_sensitive: true,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdate {
    pub enabled: bool,
    pub record_text: bool,
    pub record_image: bool,
    pub record_files: bool,
    pub max_items: u32,
    pub retention_days: u32,
    pub max_text_bytes: u64,
    pub max_image_bytes: u64,
    pub shortcut: String,
    pub excluded_apps: Vec<String>,
    pub skip_sensitive: bool,
    #[serde(default)]
    pub confirm: bool,
}

impl SettingsUpdate {
    pub fn into_settings(self, paused: bool) -> Result<Settings, ClipboardError> {
        let settings = Settings {
            enabled: self.enabled,
            paused,
            record_text: self.record_text,
            record_image: self.record_image,
            record_files: self.record_files,
            max_items: self.max_items,
            retention_days: self.retention_days,
            max_text_bytes: self.max_text_bytes,
            max_image_bytes: self.max_image_bytes,
            shortcut: self.shortcut,
            excluded_apps: self.excluded_apps,
            skip_sensitive: self.skip_sensitive,
        };
        settings.validate()?;
        Ok(settings.normalized())
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), ClipboardError> {
        if !(1..=5000).contains(&self.max_items)
            || self.retention_days > 3650
            || !(1024..=10 * 1024 * 1024).contains(&self.max_text_bytes)
            || !(1024 * 1024..=100 * 1024 * 1024).contains(&self.max_image_bytes)
            || self.shortcut.trim().is_empty()
            || self.shortcut.chars().count() > 80
            || self.excluded_apps.len() > 100
            || self
                .excluded_apps
                .iter()
                .any(|app| app.chars().count() > 256)
        {
            return Err(ClipboardError::new("invalid_input"));
        }
        Ok(())
    }

    pub fn normalized(mut self) -> Self {
        let mut apps = Vec::new();
        for app in self.excluded_apps {
            let app = app.trim().to_string();
            if !app.is_empty() && !apps.iter().any(|existing: &String| existing == &app) {
                apps.push(app);
            }
        }
        self.excluded_apps = apps;
        self.shortcut = self.shortcut.trim().to_string();
        self
    }

    pub fn allows(&self, kind: ClipKind) -> bool {
        match kind {
            ClipKind::Text => self.record_text,
            ClipKind::Image => self.record_image,
            ClipKind::Files => self.record_files,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: String,
    pub platform: String,
    pub can_monitor: bool,
    pub can_read_source_app: bool,
    pub can_global_shortcut: bool,
    pub enabled: bool,
    pub paused: bool,
    pub skipped_sensitive: u64,
    pub skipped_too_large: u64,
    pub skipped_excluded: u64,
    pub reason: Option<String>,
    pub storage_path: String,
    pub access: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Counters {
    pub sensitive: u64,
    pub too_large: u64,
    pub excluded: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyOutcome {
    pub missing: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PlatformCaps {
    pub platform: &'static str,
    pub can_monitor: bool,
    pub can_read_source_app: bool,
    pub can_global_shortcut: bool,
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_settings_outside_the_supported_range() {
        let mut settings = Settings::default();
        settings.max_items = 0;
        assert!(settings.validate().is_err());
        settings.max_items = 500;
        settings.max_text_bytes = 10;
        assert!(settings.validate().is_err());
        settings.max_text_bytes = 1024;
        settings.excluded_apps = vec!["x".repeat(257)];
        assert!(settings.validate().is_err());
    }

    #[test]
    fn normalizes_excluded_apps() {
        let mut settings = Settings::default();
        settings.excluded_apps = vec!["  Notes  ".into(), "Notes".into(), "  ".into()];
        let settings = settings.normalized();
        assert_eq!(settings.excluded_apps, vec!["Notes".to_string()]);
    }
}
