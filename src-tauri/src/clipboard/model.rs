use serde::{Deserialize, Serialize};

use super::error::ClipboardError;

pub const DEFAULT_SHORTCUT: &str = "CommandOrControl+Shift+V";

fn copy_on_select_enabled() -> bool {
    true
}

fn paste_automatically_enabled() -> bool {
    true
}

fn default_pin_shortcut() -> String {
    "CommandOrControl+P".to_string()
}

fn default_delete_shortcut() -> String {
    "CommandOrControl+Backspace".to_string()
}

pub fn default_ignored_types() -> Vec<String> {
    vec![
        "com.agilebits.onepassword".into(),
        "com.typeit4me.clipping".into(),
        "de.petermaurer.TransientPasteboardType".into(),
        "Pasteboard generator type".into(),
        "net.antelle.keeweb".into(),
    ]
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichText {
    pub plain: String,
    pub html: Option<String>,
    pub rtf: Option<Vec<u8>>,
}

impl RichText {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            plain: text.into(),
            html: None,
            rtf: None,
        }
    }

    pub fn formatted(&self) -> bool {
        self.html.as_ref().is_some_and(|html| !html.is_empty())
            || self.rtf.as_ref().is_some_and(|rtf| !rtf.is_empty())
    }
}

#[derive(Debug, Clone)]
pub enum Payload {
    Text(RichText),
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
    pub types: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMark {
    pub name: String,
    pub icon_png: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    pub id: String,
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
    pub formatted: bool,
    pub pin_key: Option<String>,
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
    #[serde(default = "copy_on_select_enabled")]
    pub copy_on_select: bool,
    #[serde(default = "paste_automatically_enabled")]
    pub paste_automatically: bool,
    #[serde(default = "default_pin_shortcut")]
    pub pin_shortcut: String,
    #[serde(default = "default_delete_shortcut")]
    pub delete_shortcut: String,
    #[serde(default = "default_ignored_types")]
    pub ignored_types: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            paused: false,
            record_text: true,
            record_image: true,
            record_files: true,
            max_items: 200,
            retention_days: 30,
            max_text_bytes: 1024 * 1024,
            max_image_bytes: 20 * 1024 * 1024,
            shortcut: DEFAULT_SHORTCUT.to_string(),
            excluded_apps: Vec::new(),
            skip_sensitive: true,
            copy_on_select: true,
            paste_automatically: true,
            pin_shortcut: default_pin_shortcut(),
            delete_shortcut: default_delete_shortcut(),
            ignored_types: default_ignored_types(),
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
    #[serde(default = "copy_on_select_enabled")]
    pub copy_on_select: bool,
    #[serde(default = "paste_automatically_enabled")]
    pub paste_automatically: bool,
    #[serde(default = "default_pin_shortcut")]
    pub pin_shortcut: String,
    #[serde(default = "default_delete_shortcut")]
    pub delete_shortcut: String,
    #[serde(default = "default_ignored_types")]
    pub ignored_types: Vec<String>,
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
            copy_on_select: self.copy_on_select,
            paste_automatically: self.paste_automatically,
            pin_shortcut: self.pin_shortcut,
            delete_shortcut: self.delete_shortcut,
            ignored_types: self.ignored_types,
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
            || self.pin_shortcut.trim().is_empty()
            || self.pin_shortcut.chars().count() > 80
            || self.delete_shortcut.trim().is_empty()
            || self.delete_shortcut.chars().count() > 80
            || self.pin_shortcut.trim() == self.delete_shortcut.trim()
            || self.pin_shortcut.trim() == self.shortcut.trim()
            || self.delete_shortcut.trim() == self.shortcut.trim()
            || self.excluded_apps.len() > 100
            || self
                .excluded_apps
                .iter()
                .any(|app| app.chars().count() > 256)
            || self.ignored_types.len() > 100
            || self
                .ignored_types
                .iter()
                .any(|kind| kind.chars().count() > 200)
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
        let mut ignored = Vec::new();
        for kind in self.ignored_types {
            let kind = kind.trim().to_string();
            if !kind.is_empty() && !ignored.iter().any(|existing: &String| existing == &kind) {
                ignored.push(kind);
            }
        }
        self.ignored_types = ignored;
        self.shortcut = self.shortcut.trim().to_string();
        self.pin_shortcut = self.pin_shortcut.trim().to_string();
        self.delete_shortcut = self.delete_shortcut.trim().to_string();
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
    pub skipped_ignored: u64,
    pub ignore_next: bool,
    pub reason: Option<String>,
    pub storage_path: String,
    pub access: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Counters {
    pub sensitive: u64,
    pub too_large: u64,
    pub excluded: u64,
    pub ignored: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyOutcome {
    pub missing: Vec<String>,
    pub pasted: bool,
    pub paste_error: Option<String>,
}

impl CopyOutcome {
    pub fn copied(missing: Vec<String>) -> Self {
        Self {
            missing,
            pasted: false,
            paste_error: None,
        }
    }
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
    fn saved_settings_without_copy_on_select_stay_enabled() {
        let mut value = serde_json::to_value(Settings::default()).expect("settings serialize");
        value
            .as_object_mut()
            .expect("settings object")
            .remove("copyOnSelect");
        value
            .as_object_mut()
            .expect("settings object")
            .remove("pasteAutomatically");
        value
            .as_object_mut()
            .expect("settings object")
            .remove("ignoredTypes");
        let settings: Settings = serde_json::from_value(value).expect("settings parse");
        assert!(settings.copy_on_select);
        assert!(settings.paste_automatically);
        assert_eq!(settings.ignored_types, default_ignored_types());
        assert_eq!(settings.pin_shortcut, "CommandOrControl+P");
        assert_eq!(settings.delete_shortcut, "CommandOrControl+Backspace");
    }

    #[test]
    fn normalizes_excluded_apps() {
        let mut settings = Settings::default();
        settings.excluded_apps = vec!["  Notes  ".into(), "Notes".into(), "  ".into()];
        let settings = settings.normalized();
        assert_eq!(settings.excluded_apps, vec!["Notes".to_string()]);
    }
}
