use serde::{Deserialize, Serialize};

use super::error::ScreenshotError;

pub const DEFAULT_SHORTCUT: &str = "CommandOrControl+Alt+A";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShotSettings {
    pub shortcut: String,
    pub record_shortcut: String,
    pub save_dir: String,
    pub format: String,
    pub jpeg_quality: u8,
    pub hide_self: bool,
    pub scroll_max_height: u32,
    #[serde(default)]
    pub scroll_auto: bool,
    #[serde(default = "default_shot_enabled")]
    pub enabled: bool,
}

impl ShotSettings {
    pub fn fresh(save_dir: String) -> Self {
        Self {
            shortcut: DEFAULT_SHORTCUT.into(),
            record_shortcut: default_record_shortcut(),
            save_dir,
            format: "png".into(),
            jpeg_quality: 90,
            hide_self: true,
            scroll_max_height: 30_000,
            scroll_auto: false,
            enabled: true,
        }
    }

    pub fn validate(&self) -> Result<(), ScreenshotError> {
        if self.shortcut.trim().is_empty()
            || self.record_shortcut.trim().is_empty()
            || self.shortcut.chars().count() > 80
            || self.record_shortcut.chars().count() > 80
            || self.shortcut.trim() == self.record_shortcut.trim()
            || self.save_dir.trim().is_empty()
            || self.save_dir.chars().count() > 1024
            || !matches!(self.format.as_str(), "png" | "jpeg")
            || !(60..=100).contains(&self.jpeg_quality)
            || !(2_000..=60_000).contains(&self.scroll_max_height)
        {
            return Err(ScreenshotError::new("invalid_input"));
        }
        Ok(())
    }
}

fn default_record_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "CommandOrControl+Shift+R".into()
    } else {
        "Alt+Shift+R".into()
    }
}

fn default_shot_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PaletteColor {
    Red,
    Yellow,
    Green,
    Blue,
    Black,
    White,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StrokeSize {
    Sm,
    Md,
    Lg,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Mark {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        size: StrokeSize,
        color: PaletteColor,
    },
    Ellipse {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        size: StrokeSize,
        color: PaletteColor,
    },
    Arrow {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        size: StrokeSize,
        color: PaletteColor,
    },
    Pen {
        points: Vec<Point>,
        size: StrokeSize,
        color: PaletteColor,
    },
    Highlight {
        points: Vec<Point>,
        size: StrokeSize,
        color: PaletteColor,
    },
    MosaicPen {
        points: Vec<Point>,
        size: StrokeSize,
    },
    MosaicRect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        size: StrokeSize,
    },
    Text {
        x: f64,
        y: f64,
        w: f64,
        text: String,
        size: StrokeSize,
        font_size: f64,
        color: PaletteColor,
    },
    Tag {
        anchor_x: f64,
        anchor_y: f64,
        text: String,
        size: StrokeSize,
        color: PaletteColor,
        side: TagSide,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TagSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Watermark {
    pub text: String,
    pub opacity: f64,
    pub color: PaletteColor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EditorDocument {
    pub monitor_index: u32,
    pub selection: Option<Rect>,
    pub marks: Vec<Mark>,
    pub watermark: Option<Watermark>,
}

impl Default for EditorDocument {
    fn default() -> Self {
        Self {
            monitor_index: 0,
            selection: None,
            marks: Vec::new(),
            watermark: None,
        }
    }
}

pub fn validate_document(
    document: &EditorDocument,
    widths: &[u32],
    heights: &[u32],
) -> Result<(), ScreenshotError> {
    let index = document.monitor_index as usize;
    if index >= widths.len() || index >= heights.len() {
        return Err(ScreenshotError::new("invalid_input"));
    }
    let width = f64::from(widths[index]);
    let height = f64::from(heights[index]);
    match &document.selection {
        None => {
            if !document.marks.is_empty() || document.watermark.is_some() {
                return Err(ScreenshotError::new("invalid_input"));
            }
        }
        Some(selection) => {
            if !inside_image(
                selection.x,
                selection.y,
                selection.w,
                selection.h,
                width,
                height,
            ) {
                return Err(ScreenshotError::new("invalid_input"));
            }
        }
    }
    if document.marks.len() > 300 {
        return Err(ScreenshotError::new("invalid_input"));
    }
    for mark in &document.marks {
        validate_mark(mark, width, height)?;
    }
    if let Some(watermark) = &document.watermark {
        if watermark.text.chars().count() > 40
            || watermark.text.trim().is_empty()
            || !(0.1..=1.0).contains(&watermark.opacity)
        {
            return Err(ScreenshotError::new("invalid_input"));
        }
    }
    Ok(())
}

fn validate_mark(mark: &Mark, width: f64, height: f64) -> Result<(), ScreenshotError> {
    match mark {
        Mark::Rect { x, y, w, h, .. }
        | Mark::Ellipse { x, y, w, h, .. }
        | Mark::MosaicRect { x, y, w, h, .. } => {
            if !finite_box(*x, *y, *w, *h, width, height) || *w < 2.0 || *h < 2.0 {
                return Err(ScreenshotError::new("invalid_input"));
            }
        }
        Mark::Arrow { x1, y1, x2, y2, .. } => {
            if !point_inside(*x1, *y1, width, height)
                || !point_inside(*x2, *y2, width, height)
                || (*x2 - *x1).hypot(*y2 - *y1) < 2.0
            {
                return Err(ScreenshotError::new("invalid_input"));
            }
        }
        Mark::Pen { points, .. }
        | Mark::Highlight { points, .. }
        | Mark::MosaicPen { points, .. } => {
            if points.len() < 2 || points.len() > 8_000 {
                return Err(ScreenshotError::new("invalid_input"));
            }
            if points
                .iter()
                .any(|point| !point_inside(point.x, point.y, width, height))
            {
                return Err(ScreenshotError::new("invalid_input"));
            }
        }
        Mark::Text {
            x,
            y,
            w,
            text,
            font_size,
            ..
        } => {
            if !point_inside(*x, *y, width, height)
                || !w.is_finite()
                || *w < 2.0
                || !font_size.is_finite()
                || !(12.0..=200.0).contains(font_size)
                || text.chars().count() > 4_000
                || text.is_empty()
            {
                return Err(ScreenshotError::new("invalid_input"));
            }
        }
        Mark::Tag {
            anchor_x,
            anchor_y,
            text,
            ..
        } => {
            if !point_inside(*anchor_x, *anchor_y, width, height)
                || text.chars().count() > 4_000
                || text.is_empty()
            {
                return Err(ScreenshotError::new("invalid_input"));
            }
        }
    }
    Ok(())
}

fn inside_image(x: f64, y: f64, w: f64, h: f64, width: f64, height: f64) -> bool {
    x.is_finite()
        && y.is_finite()
        && w.is_finite()
        && h.is_finite()
        && w >= 1.0
        && h >= 1.0
        && x >= 0.0
        && y >= 0.0
        && x + w <= width + 0.5
        && y + h <= height + 0.5
}

fn finite_box(x: f64, y: f64, w: f64, h: f64, width: f64, height: f64) -> bool {
    [x, y, w, h].iter().all(|value| value.is_finite())
        && x >= -1.0
        && y >= -1.0
        && x + w <= width + 1.0
        && y + h <= height + 1.0
}

fn point_inside(x: f64, y: f64, width: f64, height: f64) -> bool {
    x.is_finite()
        && y.is_finite()
        && x >= -1.0
        && y >= -1.0
        && x <= width + 1.0
        && y <= height + 1.0
}

#[derive(Debug, Clone)]
pub struct Editor {
    document: EditorDocument,
    version: u64,
    undo: Vec<EditorDocument>,
    redo: Vec<EditorDocument>,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            document: EditorDocument::default(),
            version: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn document(&self) -> &EditorDocument {
        &self.document
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn commit(
        &mut self,
        base_version: u64,
        document: EditorDocument,
        widths: &[u32],
        heights: &[u32],
    ) -> Result<(), ScreenshotError> {
        if base_version != self.version {
            return Err(ScreenshotError::new("invalid_input"));
        }
        validate_document(&document, widths, heights)?;
        if document == self.document {
            return Ok(());
        }
        self.undo.push(self.document.clone());
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.document = document;
        self.version += 1;
        Ok(())
    }

    pub fn undo(&mut self) {
        let Some(previous) = self.undo.pop() else {
            return;
        };
        self.redo.push(self.document.clone());
        self.document = previous;
        self.version += 1;
    }

    pub fn redo(&mut self) {
        let Some(next) = self.redo.pop() else {
            return;
        };
        self.undo.push(self.document.clone());
        self.document = next;
        self.version += 1;
    }

    pub fn reset(
        &mut self,
        document: EditorDocument,
        widths: &[u32],
        heights: &[u32],
    ) -> Result<(), ScreenshotError> {
        validate_document(&document, widths, heights)?;
        self.undo.clear();
        self.redo.clear();
        self.document = document;
        self.version += 1;
        Ok(())
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(selection: Option<Rect>) -> EditorDocument {
        EditorDocument {
            monitor_index: 0,
            selection,
            marks: Vec::new(),
            watermark: None,
        }
    }

    #[test]
    fn settings_reject_matching_shortcuts_and_bad_quality() {
        let mut settings = ShotSettings::fresh("/tmp/Pixo".into());
        assert_eq!(settings.shortcut, DEFAULT_SHORTCUT);
        assert_ne!(settings.shortcut, settings.record_shortcut);
        assert!(settings.validate().is_ok());
        assert!(settings.enabled);
        let stored = serde_json::to_string(&settings).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&stored).unwrap();
        value.as_object_mut().unwrap().remove("enabled");
        value.as_object_mut().unwrap().remove("scrollAuto");
        let restored: ShotSettings = serde_json::from_value(value).unwrap();
        assert!(restored.enabled);
        assert!(!restored.scroll_auto);
        settings.jpeg_quality = 10;
        assert!(settings.validate().is_err());
        settings.jpeg_quality = 90;
        settings.record_shortcut = settings.shortcut.clone();
        assert!(settings.validate().is_err());
    }

    #[test]
    fn stale_commit_keeps_the_document() {
        let mut editor = Editor::new();
        let next = document(Some(Rect {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 20.0,
        }));
        editor.commit(0, next, &[100], &[100]).unwrap();
        let rejected = document(Some(Rect {
            x: 1.0,
            y: 1.0,
            w: 10.0,
            h: 10.0,
        }));
        assert!(editor.commit(0, rejected, &[100], &[100]).is_err());
        assert_eq!(editor.version(), 1);
        assert_eq!(editor.document().selection.unwrap().x, 0.0);
    }

    #[test]
    fn undo_restores_the_previous_selection() {
        let mut editor = Editor::new();
        editor
            .commit(
                0,
                document(Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 10.0,
                    h: 10.0,
                })),
                &[100],
                &[100],
            )
            .unwrap();
        let version = editor.version();
        editor
            .commit(
                version,
                document(Some(Rect {
                    x: 2.0,
                    y: 2.0,
                    w: 10.0,
                    h: 10.0,
                })),
                &[100],
                &[100],
            )
            .unwrap();
        editor.undo();
        assert_eq!(editor.document().selection.unwrap().x, 0.0);
        editor.redo();
        assert_eq!(editor.document().selection.unwrap().x, 2.0);
    }

    #[test]
    fn arrow_shorter_than_two_pixels_is_rejected() {
        let mut document = document(Some(Rect {
            x: 0.0,
            y: 0.0,
            w: 50.0,
            h: 50.0,
        }));
        document.marks.push(Mark::Arrow {
            x1: 1.0,
            y1: 1.0,
            x2: 2.0,
            y2: 1.0,
            size: StrokeSize::Md,
            color: PaletteColor::Red,
        });
        assert!(validate_document(&document, &[100], &[100]).is_err());
    }

    #[test]
    fn text_font_size_stays_in_range() {
        let mut document = document(Some(Rect {
            x: 0.0,
            y: 0.0,
            w: 80.0,
            h: 40.0,
        }));
        document.marks.push(Mark::Text {
            x: 4.0,
            y: 4.0,
            w: 40.0,
            text: "你好".into(),
            size: StrokeSize::Md,
            font_size: 36.0,
            color: PaletteColor::Red,
        });
        assert!(validate_document(&document, &[100], &[100]).is_ok());
        if let Mark::Text { font_size, .. } = &mut document.marks[0] {
            *font_size = 8.0;
        }
        assert!(validate_document(&document, &[100], &[100]).is_err());
    }
}
