use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

use super::bytes::{decode_base64, decode_rgba_png, encode_jpeg};
use super::capture::{self, CapturedMonitor};
use super::clock::unique_file;
use super::error::ScreenshotError;
use super::frame;
use super::model::{Editor, EditorDocument, Rect, ShotSettings};
use super::snap::{Bounds, snap_windows};
use super::windows;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub index: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapInfo {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: u64,
    pub monitors: Vec<MonitorInfo>,
    pub windows: Vec<SnapInfo>,
    pub document: EditorDocument,
    pub version: u64,
    pub can_undo: bool,
    pub can_redo: bool,
    pub truncated: bool,
    pub mode: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotStatus {
    pub platform: String,
    pub can_capture: bool,
    pub can_snap: bool,
    pub can_scroll: bool,
    pub can_ocr: bool,
    pub can_record: bool,
    pub capture_reason: Option<String>,
    pub screen_recording: bool,
    pub session: String,
    pub save_dir: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    pub history: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub path: String,
}

struct HiddenWindow {
    label: String,
    was_visible: bool,
}

struct LiveSession {
    id: u64,
    frames: Vec<Vec<u8>>,
    monitors: Vec<MonitorInfo>,
    windows: Vec<SnapInfo>,
    editor: Editor,
    hidden: Vec<HiddenWindow>,
    previous_app: Option<String>,
    signature: Vec<(i32, i32, u32, u32)>,
    content: Vec<(u32, u32)>,
    truncated: bool,
    mode: String,
}

struct PinImage {
    png: Vec<u8>,
}

struct Inner {
    settings: Mutex<ShotSettings>,
    session: Mutex<Option<LiveSession>>,
    pins: Mutex<[Option<PinImage>; 8]>,
    pin_ready: Mutex<[Option<SyncSender<()>>; 8]>,
    shortcut: Mutex<Option<String>>,
    record_shortcut: Mutex<Option<String>>,
    app: AppHandle,
    next_id: AtomicU64,
    starting: AtomicBool,
}

pub struct ScreenshotState {
    inner: Arc<Inner>,
}

impl Clone for ScreenshotState {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

pub fn initialize(app: AppHandle) -> Result<ScreenshotState, ScreenshotError> {
    let settings = load_settings(&app)?;
    let state = ScreenshotState {
        inner: Arc::new(Inner {
            settings: Mutex::new(settings),
            session: Mutex::new(None),
            pins: Mutex::new(std::array::from_fn(|_| None)),
            pin_ready: Mutex::new(std::array::from_fn(|_| None)),
            shortcut: Mutex::new(None),
            record_shortcut: Mutex::new(None),
            app: app.clone(),
            next_id: AtomicU64::new(1),
            starting: AtomicBool::new(false),
        }),
    };
    if let Some(cache) = cache_root(&app) {
        let _ = fs::remove_dir_all(cache.join("session"));
    }
    state.register_current();
    warm_overlays(app);
    Ok(state)
}

impl ScreenshotState {
    pub fn status(&self) -> ShotStatus {
        let settings = self.lock_settings();
        let (can_capture, reason) = capture_capability();
        ShotStatus {
            platform: capture::session_kind().into(),
            can_capture,
            can_snap: can_capture,
            can_scroll: can_capture,
            can_ocr: super::ocr::available(),
            can_record: can_capture,
            capture_reason: reason,
            screen_recording: crate::permissions::screen_recording_granted(),
            session: if self
                .inner
                .session
                .lock()
                .ok()
                .is_some_and(|slot| slot.is_some())
            {
                "live".into()
            } else {
                "idle".into()
            },
            save_dir: settings.save_dir.clone(),
        }
    }

    pub fn settings(&self) -> ShotSettings {
        self.lock_settings()
    }

    pub fn bindings(&self) -> [String; 2] {
        let settings = self.lock_settings();
        [settings.shortcut.clone(), settings.record_shortcut.clone()]
    }

    pub fn update_settings(&self, incoming: ShotSettings) -> Result<ShotSettings, ScreenshotError> {
        incoming.validate()?;
        crate::shortcut::parse_shortcut(incoming.shortcut.trim())
            .map_err(|_| ScreenshotError::new("invalid_input"))?;
        crate::shortcut::parse_shortcut(incoming.record_shortcut.trim())
            .map_err(|_| ScreenshotError::new("invalid_input"))?;
        let mut next = incoming;
        next.shortcut = next.shortcut.trim().to_string();
        next.record_shortcut = next.record_shortcut.trim().to_string();
        next.save_dir = next.save_dir.trim().to_string();
        if conflicts_with_clipboard(&self.inner.app, &next.shortcut)
            || conflicts_with_clipboard(&self.inner.app, &next.record_shortcut)
        {
            return Err(ScreenshotError::new("shortcut_conflict"));
        }
        let current = self.lock_settings();
        save_settings(&self.inner.app, &next)?;
        let active = next.enabled && !capture::is_wayland();
        if let Err(error) = self.rebind_shortcuts(&next.shortcut, &next.record_shortcut, active) {
            let _ = save_settings(&self.inner.app, &current);
            let _ = self.rebind_shortcuts(
                &current.shortcut,
                &current.record_shortcut,
                current.enabled && !capture::is_wayland(),
            );
            return Err(error);
        }
        *self
            .inner
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = next.clone();
        super::scroll::set_auto(next.scroll_auto);
        Ok(next)
    }

    pub fn set_scroll_auto(&self, enabled: bool) -> Result<ShotSettings, ScreenshotError> {
        let current = self.lock_settings();
        if current.scroll_auto == enabled {
            super::scroll::set_auto(enabled);
            return Ok(current);
        }
        let mut next = current.clone();
        next.scroll_auto = enabled;
        save_settings(&self.inner.app, &next)?;
        *self
            .inner
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = next.clone();
        super::scroll::set_auto(enabled);
        Ok(next)
    }

    pub async fn pick_save_dir(&self, title: &str) -> Result<Option<String>, ScreenshotError> {
        let title = title.trim();
        if title.chars().count() > 80 {
            return Err(ScreenshotError::new("invalid_input"));
        }
        let fallback = default_save_dir(&self.inner.app);
        let start = folder_dialog_start(&self.settings().save_dir, Path::new(&fallback));
        let mut picker = self.inner.app.dialog().file().set_directory(&start);
        // macOS localizes the panel from the app bundle. A custom title would
        // stay in the in-app language instead of the system language.
        #[cfg(not(target_os = "macos"))]
        if !title.is_empty() {
            picker = picker.set_title(title);
        }
        if let Some(window) = self.inner.app.get_webview_window("main") {
            picker = picker.set_parent(&window);
        }
        // The folder panel is delivered to the main run loop. Waiting for it
        // on that same thread leaves the panel unable to run.
        let (tx, mut rx) = tauri::async_runtime::channel(1);
        picker.pick_folder(move |picked| {
            let _ = tx.blocking_send(picked);
        });
        let Some(picked) = rx.recv().await.flatten() else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(|error| {
            eprintln!("screenshot folder: {error}");
            ScreenshotError::new("invalid_input")
        })?;
        if !path.is_dir() {
            return Err(ScreenshotError::new("invalid_input"));
        }
        Ok(Some(path.to_string_lossy().into_owned()))
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<ShotSettings, ScreenshotError> {
        let current = self.lock_settings();
        if current.enabled == enabled {
            return Ok(current);
        }
        let mut next = current.clone();
        next.enabled = enabled;
        save_settings(&self.inner.app, &next)?;
        let active = enabled && !capture::is_wayland();
        if let Err(error) = self.rebind_shortcuts(&next.shortcut, &next.record_shortcut, active) {
            let _ = save_settings(&self.inner.app, &current);
            let _ = self.rebind_shortcuts(
                &current.shortcut,
                &current.record_shortcut,
                current.enabled && !capture::is_wayland(),
            );
            return Err(error);
        }
        *self
            .inner
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = next.clone();
        if !enabled && !super::record::is_active() {
            let _ = self.cancel();
        }
        let _ = self.inner.app.emit("screenshot://enabled", enabled);
        Ok(next)
    }

    pub fn request_access(&self) -> ShotStatus {
        if !crate::permissions::screen_recording_granted() {
            let _ = crate::permissions::request_screen_recording();
        }
        self.status()
    }

    pub fn start(&self, mode: &str, hide_self: bool) -> Result<SessionView, ScreenshotError> {
        if super::record::is_active() {
            return Err(ScreenshotError::new("busy"));
        }
        if !self.lock_settings().enabled {
            return Err(ScreenshotError::new("unsupported"));
        }
        if mode != "capture" && mode != "record" {
            return Err(ScreenshotError::new("invalid_input"));
        }
        if self.inner.starting.swap(true, Ordering::SeqCst) {
            return Err(ScreenshotError::new("busy"));
        }
        let replacing = self
            .inner
            .session
            .lock()
            .ok()
            .is_some_and(|slot| slot.is_some());
        let result = self.start_live(hide_self, mode);
        self.inner.starting.store(false, Ordering::SeqCst);
        if result.is_ok() && !replacing {
            super::ocr::clear_pending_cancel();
        }
        result
    }

    pub fn start_record(
        &self,
        monitor_index: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        format: &str,
    ) -> Result<(), ScreenshotError> {
        let format = super::record::RecordFormat::parse(format)
            .ok_or_else(|| ScreenshotError::new("invalid_input"))?;
        ensure_capture_allowed()?;
        if super::record::is_active() {
            return Err(ScreenshotError::new("busy"));
        }
        if !x.is_finite()
            || !y.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || x < 0.0
            || y < 0.0
            || width < 8.0
            || height < 8.0
        {
            return Err(ScreenshotError::new("invalid_input"));
        }
        let save_dir = self.lock_settings().save_dir.clone();
        let session = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let live = session
            .as_ref()
            .ok_or_else(|| ScreenshotError::new("cancelled"))?;
        let monitor = live
            .monitors
            .get(monitor_index as usize)
            .ok_or_else(|| ScreenshotError::new("invalid_input"))?;
        if x + width > f64::from(monitor.width) + 1.0
            || y + height > f64::from(monitor.height) + 1.0
        {
            return Err(ScreenshotError::new("invalid_input"));
        }
        let plan = super::record::RecordPlan {
            monitor_index: monitor_index as usize,
            x: x.round().max(0.0) as u32,
            y: y.round().max(0.0) as u32,
            width: width.round().max(1.0) as u32,
            height: height.round().max(1.0) as u32,
            scale: monitor.scale_factor.max(1.0),
            monitor_x: monitor.x,
            monitor_y: monitor.y,
            monitor_width: monitor.width,
            monitor_height: monitor.height,
            save_dir,
            format,
        };
        drop(session);
        super::record::start(self.inner.app.clone(), plan)
    }

    pub fn current(&self) -> Result<SessionView, ScreenshotError> {
        let session = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        session
            .as_ref()
            .map(LiveSession::view)
            .ok_or_else(|| ScreenshotError::new("cancelled"))
    }

    pub fn frame(&self, index: u32) -> Result<Vec<u8>, ScreenshotError> {
        let session = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = session
            .as_ref()
            .ok_or_else(|| ScreenshotError::new("cancelled"))?;
        session
            .frames
            .get(index as usize)
            .cloned()
            .ok_or_else(|| ScreenshotError::new("capture_failed"))
    }

    pub fn scroll_capture(
        &self,
        label: &str,
        monitor_index: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<(), ScreenshotError> {
        if capture::is_wayland() {
            return Err(ScreenshotError::with_params(
                "unsupported",
                serde_json::json!({ "reason": "wayland" }),
            ));
        }
        let area = scroll_area(x, y, width, height).ok_or_else(|| {
            ScreenshotError::with_params("invalid_input", serde_json::json!({ "reason": "scroll" }))
        })?;
        let (max_height, scroll_auto) = {
            let settings = self.lock_settings();
            (settings.scroll_max_height, settings.scroll_auto)
        };
        let monitor = {
            let session = self
                .inner
                .session
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let session = session
                .as_ref()
                .ok_or_else(|| ScreenshotError::new("cancelled"))?;
            let monitor = session
                .monitors
                .get(monitor_index as usize)
                .cloned()
                .ok_or_else(|| ScreenshotError::new("invalid_input"))?;
            if area.0.saturating_add(area.2) > monitor.width
                || area.1.saturating_add(area.3) > monitor.height
            {
                return Err(ScreenshotError::with_params(
                    "invalid_input",
                    serde_json::json!({ "reason": "scroll" }),
                ));
            }
            monitor
        };
        super::scroll::clear_stop();
        super::scroll::set_auto(scroll_auto);
        let app = &self.inner.app;
        let _ = label;
        let generation = super::scroll::next_generation();
        let _ = app.emit(
            "screenshot://scroll-begin",
            serde_json::json!({ "generation": generation }),
        );
        let parked = park_scroll_hud(app, &monitor, area);
        let mut restored = ScrollRestore {
            app,
            parked,
            retain: false,
        };
        if !parked {
            return Err(ScreenshotError::new("capture_failed"));
        }
        let cursor = scroll_cursor(&monitor, area);
        let strip = super::scroll::capture_strip(
            monitor_index as usize,
            area.0,
            area.1,
            area.2,
            area.3,
            max_height,
            cursor,
            |rgba, width, height| {
                let preview = super::scroll::preview_png(rgba, width, height)
                    .map(|png| super::scroll::encode_base64(&png));
                let _ = app.emit(
                    "screenshot://scroll",
                    serde_json::json!({
                        "generation": generation,
                        "height": height,
                        "preview": preview,
                    }),
                );
            },
        );
        let strip = strip?;
        let encoded = frame::encode(strip.width, strip.height, &strip.rgba)?;
        let mut slot = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = slot
            .as_mut()
            .ok_or_else(|| ScreenshotError::new("cancelled"))?;
        let index = monitor_index as usize;
        if index >= session.frames.len() || index >= session.content.len() {
            return Err(ScreenshotError::new("capture_failed"));
        }
        session.frames[index] = encoded;
        session.content[index] = (strip.width, strip.height);
        let document = EditorDocument {
            monitor_index,
            selection: Some(Rect {
                x: 0.0,
                y: 0.0,
                w: f64::from(strip.width),
                h: f64::from(strip.height),
            }),
            marks: Vec::new(),
            watermark: None,
        };
        let widths: Vec<u32> = session.content.iter().map(|(item, _)| *item).collect();
        let heights: Vec<u32> = session.content.iter().map(|(_, item)| *item).collect();
        session.editor.reset(document, &widths, &heights)?;
        let view = session.view();
        drop(slot);
        let _ = self.inner.app.emit(
            "screenshot://editor",
            serde_json::json!({ "version": view.version }),
        );
        let (left, top, width, height) = result_frame(&monitor, strip.width, strip.height);
        if !super::windows::present_scroll_result(app, left, top, width, height) {
            return Err(ScreenshotError::new("capture_failed"));
        }
        let preview =
            super::scroll::preview_png_within(&strip.rgba, strip.width, strip.height, 1400, 1400)
                .map(|png| super::scroll::encode_base64(&png));
        let _ = app.emit(
            "screenshot://scroll-result",
            serde_json::json!({
                "generation": generation,
                "preview": preview,
                "width": strip.width,
                "height": strip.height,
            }),
        );
        restored.retain = true;
        Ok(())
    }

    pub fn commit(
        &self,
        base_version: u64,
        document: EditorDocument,
    ) -> Result<SessionView, ScreenshotError> {
        let mut slot = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = slot
            .as_mut()
            .ok_or_else(|| ScreenshotError::new("cancelled"))?;
        if let Some(signature) = capture::monitor_signature() {
            if signature != session.signature {
                let hidden = std::mem::take(&mut session.hidden);
                let previous = session.previous_app.take();
                *slot = None;
                drop(slot);
                self.end_overlays(hidden, previous);
                self.emit_session("cancelled", Some("display_changed"));
                return Err(ScreenshotError::with_params(
                    "capture_failed",
                    serde_json::json!({ "reason": "display_changed" }),
                ));
            }
        }
        let widths: Vec<u32> = session.content.iter().map(|(width, _)| *width).collect();
        let heights: Vec<u32> = session.content.iter().map(|(_, height)| *height).collect();
        session
            .editor
            .commit(base_version, document, &widths, &heights)?;
        let view = session.view();
        drop(slot);
        let _ = self.inner.app.emit(
            "screenshot://editor",
            serde_json::json!({ "version": view.version }),
        );
        Ok(view)
    }

    pub fn undo(&self) -> Result<SessionView, ScreenshotError> {
        self.step(true)
    }

    pub fn redo(&self) -> Result<SessionView, ScreenshotError> {
        self.step(false)
    }

    pub fn cancel(&self) -> Result<(), ScreenshotError> {
        super::scroll::request_stop();
        super::ocr::cancel_recognition();
        self.finish_session(false)?;
        self.emit_session("cancelled", None);
        Ok(())
    }

    pub fn copy_png(&self, png_base64: &str) -> Result<CopyResult, ScreenshotError> {
        let history = self.write_clipboard(png_base64)?;
        Ok(CopyResult { history })
    }

    pub fn copy_frame(&self) -> Result<CopyResult, ScreenshotError> {
        let png = self.frame_png()?;
        let history = self.store_png(&png)?;
        Ok(CopyResult { history })
    }

    pub fn save_frame(&self) -> Result<SaveResult, ScreenshotError> {
        let png = self.frame_png()?;
        self.save_encoded(&png)
    }

    fn frame_png(&self) -> Result<Vec<u8>, ScreenshotError> {
        let rgba = {
            let slot = self
                .inner
                .session
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let session = slot
                .as_ref()
                .ok_or_else(|| ScreenshotError::new("cancelled"))?;
            let index = session.editor.document().monitor_index as usize;
            let bytes = session
                .frames
                .get(index)
                .ok_or_else(|| ScreenshotError::new("capture_failed"))?;
            let (width, height, _) = frame::header(bytes)?;
            (width, height, bytes[16..].to_vec())
        };
        super::bytes::encode_png(rgba.0, rgba.1, &rgba.2)
    }

    pub fn save_png(&self, png_base64: &str) -> Result<SaveResult, ScreenshotError> {
        let bytes = decode_base64(png_base64)?;
        self.save_encoded(&bytes)
    }

    fn save_encoded(&self, bytes: &[u8]) -> Result<SaveResult, ScreenshotError> {
        let (width, height, rgba) = decode_rgba_png(bytes)?;
        let settings = self.lock_settings();
        let directory = PathBuf::from(&settings.save_dir);
        fs::create_dir_all(&directory).map_err(|error| {
            eprintln!("screenshot directory: {error}");
            ScreenshotError::new("storage")
        })?;
        let path = if settings.format == "jpeg" {
            let path = unique_file(&directory, "jpg");
            let encoded = encode_jpeg(width, height, &rgba, settings.jpeg_quality)?;
            fs::write(&path, encoded).map_err(|error| {
                eprintln!("screenshot save: {error}");
                ScreenshotError::new("storage")
            })?;
            path
        } else {
            let path = unique_file(&directory, "png");
            fs::write(&path, bytes).map_err(|error| {
                eprintln!("screenshot save: {error}");
                ScreenshotError::new("storage")
            })?;
            path
        };
        Ok(SaveResult {
            path: path.to_string_lossy().into_owned(),
        })
    }

    pub fn pin(
        &self,
        png_base64: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        scale_factor: f64,
    ) -> Result<u8, ScreenshotError> {
        let bytes = decode_base64(png_base64)?;
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(ScreenshotError::new("invalid_input"));
        }
        let slot = {
            let mut pins = self
                .inner
                .pins
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let slot = pins
                .iter()
                .position(|pin| pin.is_none())
                .ok_or_else(|| ScreenshotError::new("pin_limit"))?;
            pins[slot] = Some(PinImage { png: bytes });
            slot
        };
        let (sender, ready) = sync_channel(1);
        {
            let mut waiters = self
                .inner
                .pin_ready
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            waiters[slot] = Some(sender);
        }
        let shown = slot as u8 + 1;
        let pixel_width = width.round().max(1.0) as u32;
        let pixel_height = height.round().max(1.0) as u32;
        if let Err(error) = windows::show_pin(
            &self.inner.app,
            shown,
            x.round() as i32,
            y.round() as i32,
            pixel_width,
            pixel_height,
            scale_factor,
        ) {
            self.clear_pin(shown);
            return Err(error);
        }
        let _ = self.inner.app.emit(
            "screenshot://pin-show",
            serde_json::json!({ "slot": shown }),
        );
        // The overlay stays up until the warmed pin has painted this image.
        let _ = ready.recv_timeout(Duration::from_secs(4));
        Ok(shown)
    }

    pub fn pin_ready(&self, slot: u8) {
        let Some(sender) = self.take_pin_waiter(slot) else {
            return;
        };
        let _ = sender.send(());
    }

    fn take_pin_waiter(&self, slot: u8) -> Option<SyncSender<()>> {
        let index = slot.wrapping_sub(1) as usize;
        let mut waiters = self
            .inner
            .pin_ready
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        waiters.get_mut(index)?.take()
    }

    fn clear_pin(&self, slot: u8) {
        let index = slot.wrapping_sub(1) as usize;
        let mut pins = self
            .inner
            .pins
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(pin) = pins.get_mut(index) {
            *pin = None;
        }
        drop(pins);
        let _ = self.take_pin_waiter(slot);
    }

    pub fn pin_png(&self, slot: u8) -> Result<Vec<u8>, ScreenshotError> {
        let pins = self
            .inner
            .pins
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        pins.get(slot.wrapping_sub(1) as usize)
            .and_then(|pin| pin.as_ref().map(|pin| pin.png.clone()))
            .ok_or_else(|| ScreenshotError::new("not_found"))
    }

    pub fn pin_copy(&self, slot: u8) -> Result<CopyResult, ScreenshotError> {
        let png = self.pin_png(slot)?;
        let history = self.store_png(&png)?;
        Ok(CopyResult { history })
    }

    pub fn pin_close(&self, slot: u8) -> Result<(), ScreenshotError> {
        let mut pins = self
            .inner
            .pins
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let index = slot.wrapping_sub(1) as usize;
        if index >= pins.len() || pins[index].is_none() {
            return Err(ScreenshotError::new("not_found"));
        }
        pins[index] = None;
        drop(pins);
        let _ = self.take_pin_waiter(slot);
        let _ = self.inner.app.emit(
            "screenshot://pin-clear",
            serde_json::json!({ "slot": slot }),
        );
        windows::conceal_pin(&self.inner.app, slot);
        Ok(())
    }

    pub fn finish(&self) -> Result<(), ScreenshotError> {
        self.finish_session(true)?;
        self.emit_session("finished", None);
        Ok(())
    }

    fn start_live(&self, hide_self: bool, mode: &str) -> Result<SessionView, ScreenshotError> {
        ensure_capture_allowed()?;
        let previous_app = windows::other_front_bundle(&self.inner.app);
        if self
            .inner
            .session
            .lock()
            .ok()
            .is_some_and(|slot| slot.is_some())
        {
            let _ = self.cancel();
        }
        let hidden = hide_own_windows(&self.inner.app, hide_self);
        std::thread::sleep(Duration::from_millis(32));
        let (captured, truncated) = match capture::capture_monitors() {
            Ok(captured) => captured,
            Err(error) => {
                give_back(&self.inner.app, &hidden, previous_app.as_deref());
                return Err(error);
            }
        };
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let monitors = monitor_infos(&captured);
        let mut frames = Vec::with_capacity(captured.len());
        for monitor in &captured {
            match frame::encode(monitor.width, monitor.height, &monitor.rgba) {
                Ok(bytes) => frames.push(bytes),
                Err(error) => {
                    give_back(&self.inner.app, &hidden, previous_app.as_deref());
                    return Err(error);
                }
            }
        }
        drop(captured);
        let signature = capture::monitor_signature().unwrap_or_else(|| {
            monitors
                .iter()
                .map(|monitor| (monitor.x, monitor.y, monitor.width, monitor.height))
                .collect()
        });
        let mut overlays = Vec::with_capacity(monitors.len());
        for (index, monitor) in monitors.iter().enumerate() {
            match windows::open_overlay(
                &self.inner.app,
                index,
                monitor.x,
                monitor.y,
                monitor.width,
                monitor.height,
                monitor.scale_factor,
            ) {
                Ok(label) => overlays.push(label),
                Err(error) => {
                    give_back(&self.inner.app, &hidden, previous_app.as_deref());
                    return Err(error);
                }
            }
        }
        let session = LiveSession {
            id,
            frames,
            monitors: monitors.clone(),
            windows: Vec::new(),
            editor: Editor::new(),
            hidden,
            previous_app,
            signature,
            content: monitors
                .iter()
                .map(|monitor| (monitor.width, monitor.height))
                .collect(),
            truncated,
            mode: mode.to_string(),
        };
        windows::arm_labels(&self.inner.app, &overlays, &session.frames, id);
        let view = session.view();
        *self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(session);
        self.emit_session("live", None);
        publish_windows(Arc::clone(&self.inner), id, monitors);
        Ok(view)
    }

    fn step(&self, undo: bool) -> Result<SessionView, ScreenshotError> {
        let mut slot = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = slot
            .as_mut()
            .ok_or_else(|| ScreenshotError::new("cancelled"))?;
        if undo {
            session.editor.undo();
        } else {
            session.editor.redo();
        }
        let view = session.view();
        drop(slot);
        let _ = self.inner.app.emit(
            "screenshot://editor",
            serde_json::json!({ "version": view.version }),
        );
        Ok(view)
    }

    fn write_clipboard(&self, png_base64: &str) -> Result<String, ScreenshotError> {
        let bytes = decode_base64(png_base64)?;
        self.store_png(&bytes)
    }

    fn store_png(&self, png: &[u8]) -> Result<String, ScreenshotError> {
        let clipboard = self
            .inner
            .app
            .try_state::<crate::clipboard::ClipboardState>()
            .ok_or_else(|| ScreenshotError::new("storage"))?;
        match clipboard.record_produced_png(png) {
            Ok(history) => Ok(history.as_str().to_string()),
            Err(error) => Err(ScreenshotError::new(error.code)),
        }
    }

    fn finish_session(&self, _keep_pins: bool) -> Result<(), ScreenshotError> {
        let session = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        let Some(session) = session else {
            return Ok(());
        };
        self.end_overlays(session.hidden, session.previous_app);
        Ok(())
    }

    fn end_overlays(&self, hidden: Vec<HiddenWindow>, previous: Option<String>) {
        give_back(&self.inner.app, &hidden, previous.as_deref());
    }

    fn emit_session(&self, phase: &str, code: Option<&str>) {
        let _ = self.inner.app.emit(
            "screenshot://session",
            serde_json::json!({ "phase": phase, "code": code }),
        );
    }

    fn lock_settings(&self) -> ShotSettings {
        self.inner
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn register_current(&self) {
        let settings = self.lock_settings();
        if let Err(error) = self.rebind_shortcuts(
            &settings.shortcut,
            &settings.record_shortcut,
            settings.enabled && !capture::is_wayland(),
        ) {
            eprintln!("screenshot shortcut: {error}");
        }
    }

    fn rebind_shortcuts(
        &self,
        capture: &str,
        record: &str,
        active: bool,
    ) -> Result<(), ScreenshotError> {
        self.sync_shortcut(capture, false, super::hotkey::HotkeyKind::Capture)?;
        self.sync_shortcut(record, false, super::hotkey::HotkeyKind::Record)?;
        if !active {
            return Ok(());
        }
        self.sync_shortcut(capture, true, super::hotkey::HotkeyKind::Capture)?;
        if let Err(error) = self.sync_shortcut(record, true, super::hotkey::HotkeyKind::Record) {
            let _ = self.sync_shortcut(capture, false, super::hotkey::HotkeyKind::Capture);
            return Err(error);
        }
        Ok(())
    }

    fn sync_shortcut(
        &self,
        desired: &str,
        active: bool,
        kind: super::hotkey::HotkeyKind,
    ) -> Result<(), ScreenshotError> {
        let slot = match kind {
            super::hotkey::HotkeyKind::Capture => &self.inner.shortcut,
            super::hotkey::HotkeyKind::Record => &self.inner.record_shortcut,
        };
        let mut registered = slot.lock().unwrap_or_else(|error| error.into_inner());
        super::hotkey::sync(&self.inner.app, desired, active, &mut registered, kind)
    }
}

impl LiveSession {
    fn view(&self) -> SessionView {
        SessionView {
            id: self.id,
            monitors: self.monitors.clone(),
            windows: self.windows.clone(),
            document: self.editor.document().clone(),
            version: self.editor.version(),
            can_undo: self.editor.can_undo(),
            can_redo: self.editor.can_redo(),
            truncated: self.truncated,
            mode: self.mode.clone(),
        }
    }
}

fn ensure_capture_allowed() -> Result<(), ScreenshotError> {
    if capture::is_wayland() {
        return Err(ScreenshotError::with_params(
            "unsupported",
            serde_json::json!({ "reason": "wayland" }),
        ));
    }
    if crate::permissions::screen_recording_granted() {
        return Ok(());
    }
    let accepted = crate::permissions::request_screen_recording();
    if crate::permissions::screen_recording_granted() {
        return Ok(());
    }
    if accepted {
        Err(ScreenshotError::with_params(
            "permission_denied",
            serde_json::json!({ "reason": "relaunch_required" }),
        ))
    } else {
        Err(ScreenshotError::new("permission_denied"))
    }
}

fn capture_capability() -> (bool, Option<String>) {
    if capture::is_wayland() {
        return (false, Some("wayland".into()));
    }
    if !crate::permissions::screen_recording_granted() {
        return (false, Some("permission_denied".into()));
    }
    (true, None)
}

fn hide_own_windows(app: &AppHandle, hide: bool) -> Vec<HiddenWindow> {
    let mut hidden = Vec::new();
    for (label, window) in app.webview_windows() {
        if label.starts_with("screenshot-overlay-")
            || label.starts_with("screenshot-pin-")
            || label.starts_with("screenshot-scroll")
        {
            continue;
        }
        let visible = window.is_visible().unwrap_or(false);
        if hide && visible {
            let _ = window.hide();
        }
        hidden.push(HiddenWindow {
            label,
            was_visible: visible,
        });
    }
    hidden
}

fn give_back(app: &AppHandle, hidden: &[HiddenWindow], previous: Option<&str>) {
    let windows: Vec<(String, bool)> = hidden
        .iter()
        .map(|window| (window.label.clone(), window.was_visible))
        .collect();
    windows::settle_after_shot(app, &windows, previous);
}

fn monitor_infos(captured: &[CapturedMonitor]) -> Vec<MonitorInfo> {
    captured
        .iter()
        .enumerate()
        .map(|(index, monitor)| MonitorInfo {
            index: index as u32,
            x: monitor.origin_x,
            y: monitor.origin_y,
            width: monitor.width,
            height: monitor.height,
            scale_factor: monitor.scale_factor,
        })
        .collect()
}

fn publish_windows(inner: Arc<Inner>, id: u64, monitors: Vec<MonitorInfo>) {
    std::thread::spawn(move || {
        let windows = snap_infos(&monitors);
        let mut slot = inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(session) = slot.as_mut() else {
            return;
        };
        if session.id != id {
            return;
        }
        session.windows = windows;
        drop(slot);
        let _ = inner
            .app
            .emit("screenshot://windows", serde_json::json!({ "id": id }));
    });
}

fn warm_overlays(app: AppHandle) {
    if capture::is_wayland() {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        let layout = capture::monitor_layout();
        if layout.is_empty() {
            return;
        }
        let ui = app.clone();
        let _ = app.run_on_main_thread(move || {
            for (index, monitor) in layout.iter().enumerate() {
                let _ = windows::warm_overlay(
                    &ui,
                    index,
                    monitor.origin_x,
                    monitor.origin_y,
                    monitor.width,
                    monitor.height,
                    monitor.scale_factor,
                );
            }
        });
    });
}

fn snap_infos(monitors: &[MonitorInfo]) -> Vec<SnapInfo> {
    let own_pid = std::process::id();
    let listed = capture::list_windows();
    let mut infos = Vec::new();
    for monitor in monitors {
        let bounds = Bounds {
            x: monitor.x,
            y: monitor.y,
            width: monitor.width,
            height: monitor.height,
        };
        let pixels = listed
            .iter()
            .map(|window| capture::window_in_pixels(window, monitor.scale_factor))
            .collect::<Vec<_>>();
        for window in snap_windows(&pixels, bounds, own_pid) {
            infos.push(SnapInfo {
                x: window.x,
                y: window.y,
                width: window.width,
                height: window.height,
            });
        }
    }
    infos
}

fn cache_root(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_cache_dir()
        .ok()
        .map(|path| path.join("screenshot"))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, ScreenshotError> {
    let root = app.path().app_data_dir().map_err(|error| {
        eprintln!("screenshot settings path: {error}");
        ScreenshotError::new("storage")
    })?;
    Ok(root.join("screenshot").join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Result<ShotSettings, ScreenshotError> {
    let path = settings_path(app)?;
    if let Ok(bytes) = fs::read(&path) {
        if let Ok(mut settings) = serde_json::from_slice::<ShotSettings>(&bytes) {
            if settings.save_dir.trim().is_empty() {
                settings.save_dir = default_save_dir(app);
                let _ = save_settings(app, &settings);
            }
            if settings.validate().is_ok() {
                return Ok(settings);
            }
        }
    }
    let save_dir = default_save_dir(app);
    let settings = ShotSettings::fresh(save_dir);
    let _ = save_settings(app, &settings);
    Ok(settings)
}

fn save_settings(app: &AppHandle, settings: &ShotSettings) -> Result<(), ScreenshotError> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| ScreenshotError::new("storage"))?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|_| ScreenshotError::new("storage"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes).map_err(|_| ScreenshotError::new("storage"))?;
    fs::rename(&temporary, &path).map_err(|_| ScreenshotError::new("storage"))?;
    Ok(())
}

fn default_save_dir(app: &AppHandle) -> String {
    let pictures = app
        .path()
        .picture_dir()
        .unwrap_or_else(|_| app.path().home_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let directory = pictures.join("Pixo");
    let _ = fs::create_dir_all(&directory);
    directory.to_string_lossy().into_owned()
}

pub(crate) fn folder_dialog_start(current: &str, fallback: &Path) -> PathBuf {
    let trimmed = current.trim();
    let candidate = PathBuf::from(trimmed);
    if !trimmed.is_empty() && candidate.is_dir() {
        return candidate;
    }
    if let Some(parent) = candidate.parent() {
        if !parent.as_os_str().is_empty() && parent.is_dir() {
            return parent.to_path_buf();
        }
    }
    if fallback.is_dir() {
        return fallback.to_path_buf();
    }
    if let Some(parent) = fallback.parent() {
        if parent.is_dir() {
            return parent.to_path_buf();
        }
    }
    fallback.to_path_buf()
}

fn conflicts_with_clipboard(app: &AppHandle, shortcut: &str) -> bool {
    app.try_state::<crate::clipboard::ClipboardState>()
        .is_some_and(|clipboard| {
            clipboard
                .shortcut_bindings()
                .iter()
                .any(|binding| binding == shortcut)
        })
}

pub fn conflicts_with(app: &AppHandle, shortcuts: &[&str]) -> bool {
    app.try_state::<ScreenshotState>().is_some_and(|state| {
        let bindings = state.bindings();
        shortcuts
            .iter()
            .any(|shortcut| bindings.iter().any(|binding| binding == *shortcut))
    })
}

pub fn capture_from_hotkey(app: &AppHandle) {
    open_from_hotkey(app, "capture");
}

pub fn record_from_hotkey(app: &AppHandle) {
    open_from_hotkey(app, "record");
}

fn open_from_hotkey(app: &AppHandle, mode: &str) {
    let Some(state) = app.try_state::<ScreenshotState>() else {
        return;
    };
    let hide = state.settings().hide_self;
    if let Err(error) = state.start(mode, hide) {
        eprintln!("screenshot start: {}", error.code);
        let _ = app.emit(
            "screenshot://session",
            serde_json::json!({ "phase": "failed", "code": error.code, "params": error.params }),
        );
    }
}

fn scroll_area(x: f64, y: f64, width: f64, height: f64) -> Option<(u32, u32, u32, u32)> {
    if ![x, y, width, height].iter().all(|value| value.is_finite()) {
        return None;
    }
    if width < 32.0 || height < 48.0 || x < -0.5 || y < -0.5 {
        return None;
    }
    Some((
        x.round().max(0.0) as u32,
        y.round().max(0.0) as u32,
        width.round() as u32,
        height.round() as u32,
    ))
}

fn scroll_cursor(monitor: &MonitorInfo, area: (u32, u32, u32, u32)) -> super::scroll::Cursor {
    let left = f64::from(monitor.x) + f64::from(area.0);
    let top = f64::from(monitor.y) + f64::from(area.1);
    let right = left + f64::from(area.2);
    let bottom = top + f64::from(area.3);
    let scale = if cfg!(target_os = "macos") {
        monitor.scale_factor.max(1.0)
    } else {
        1.0
    };
    let point = |value: f64| value / scale;
    super::scroll::Cursor {
        x: point((left + right) / 2.0),
        y: point((top + bottom) / 2.0),
        left: point(left),
        top: point(top),
        right: point(right),
        bottom: point(bottom),
    }
}

fn result_frame(monitor: &MonitorInfo, image_w: u32, image_h: u32) -> (f64, f64, f64, f64) {
    let scale = monitor.scale_factor.max(1.0);
    let monitor_w = f64::from(monitor.width) / scale;
    let monitor_h = f64::from(monitor.height) / scale;
    let max_w = (monitor_w - 48.0).max(280.0).min(960.0);
    let max_h = (monitor_h - 48.0).max(280.0).min(1040.0);
    // The result footer is a single 40px line. Extra height becomes a black
    // band under the picture because the image is top-aligned in that space.
    let chrome = 40.0;
    let content_max_h = (max_h - chrome).max(120.0);
    let aspect = f64::from(image_w.max(1)) / f64::from(image_h.max(1));
    let mut content_w = max_w;
    let mut content_h = content_w / aspect;
    if content_h > content_max_h {
        content_h = content_max_h;
        content_w = (content_h * aspect).min(max_w);
    }
    let window_w = content_w.clamp(180.0, max_w);
    let window_h = content_h + chrome;
    let left = f64::from(monitor.x) / scale + (monitor_w - window_w) / 2.0;
    let top = f64::from(monitor.y) / scale + (monitor_h - window_h) / 2.0;
    (left, top, window_w, window_h)
}

fn park_scroll_hud(app: &AppHandle, monitor: &MonitorInfo, area: (u32, u32, u32, u32)) -> bool {
    let scale = monitor.scale_factor.max(1.0);
    let hud_height = ((f64::from(monitor.height) / scale) - 24.0).clamp(180.0, 420.0);
    let hud_w = capture::scale_len(260, scale);
    let hud_h = capture::scale_len(hud_height.round() as u32, scale);
    let gap = capture::scale_len(12, scale) as i32;
    let (x, y) = preview_origin(monitor, area, hud_w, hud_h, gap);
    let edges = super::record::selection_frame(
        monitor.x,
        monitor.y,
        monitor.width,
        monitor.height,
        f64::from(area.0),
        f64::from(area.1),
        f64::from(area.2),
        f64::from(area.3),
        scale,
    );
    super::windows::open_scroll_hud(
        app,
        f64::from(x) / scale,
        f64::from(y) / scale,
        260.0,
        hud_height,
        edges,
    )
    .is_ok()
}

fn preview_origin(
    monitor: &MonitorInfo,
    area: (u32, u32, u32, u32),
    hud_w: u32,
    hud_h: u32,
    gap: i32,
) -> (i32, i32) {
    let hud_w = hud_w as i32;
    let hud_h = hud_h as i32;
    let selection_x = monitor.x + area.0 as i32;
    let selection_y = monitor.y + area.1 as i32;
    let monitor_right = monitor.x + monitor.width as i32;
    let monitor_bottom = monitor.y + monitor.height as i32;
    let y = selection_y.clamp(monitor.y, (monitor_bottom - hud_h).max(monitor.y));
    let right = selection_x + area.2 as i32 + gap;
    if right + hud_w <= monitor_right {
        return (right, y);
    }
    let left = selection_x - gap - hud_w;
    if left >= monitor.x {
        return (left, y);
    }
    hud_origin(monitor, area, hud_w as u32, hud_h as u32, gap)
}

fn hud_origin(
    monitor: &MonitorInfo,
    area: (u32, u32, u32, u32),
    hud_w: u32,
    hud_h: u32,
    gap: i32,
) -> (i32, i32) {
    let hud_w = hud_w as i32;
    let hud_h = hud_h as i32;
    let centered = monitor.x + (monitor.width as i32 - hud_w) / 2;
    let top = monitor.y + gap;
    let bottom = monitor.y + monitor.height as i32 - hud_h - gap;
    let selection = (
        monitor.x + area.0 as i32,
        monitor.y + area.1 as i32,
        area.2 as i32,
        area.3 as i32,
    );
    if !rects_overlap(
        centered,
        top,
        hud_w,
        hud_h,
        selection.0,
        selection.1,
        selection.2,
        selection.3,
    ) {
        (centered, top)
    } else {
        (centered, bottom.max(monitor.y))
    }
}

fn rects_overlap(ax: i32, ay: i32, aw: i32, ah: i32, bx: i32, by: i32, bw: i32, bh: i32) -> bool {
    ax < bx.saturating_add(bw)
        && ax.saturating_add(aw) > bx
        && ay < by.saturating_add(bh)
        && ay.saturating_add(ah) > by
}

struct ScrollRestore<'a> {
    app: &'a AppHandle,
    parked: bool,
    retain: bool,
}

impl Drop for ScrollRestore<'_> {
    fn drop(&mut self) {
        if self.parked && !self.retain {
            super::windows::close_scroll_hud(self.app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::folder_dialog_start;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn folder_dialog_starts_in_the_current_or_parent_directory() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("pixo-folder-{stamp}"));
        let current = root.join("current");
        let parent = root.join("parent");
        let fallback = root.join("fallback");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&parent).unwrap();
        fs::create_dir_all(&fallback).unwrap();
        assert_eq!(
            folder_dialog_start(&current.to_string_lossy(), &fallback),
            current
        );
        assert_eq!(
            folder_dialog_start(&parent.join("later").to_string_lossy(), &fallback),
            parent
        );
        assert_eq!(
            folder_dialog_start(
                &root.join("gone").join("later").to_string_lossy(),
                &fallback
            ),
            fallback
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn result_window_hugs_the_picture_and_one_footer_line() {
        let monitor = super::MonitorInfo {
            index: 0,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
        };
        let (_, _, width, height) = super::result_frame(&monitor, 800, 400);
        assert!((width - 960.0).abs() < 0.5, "{width}");
        assert!((height - 520.0).abs() < 0.5, "{height}");
        let (_, _, short_w, short_h) = super::result_frame(&monitor, 800, 50);
        assert!((short_w - 960.0).abs() < 0.5, "{short_w}");
        assert!((short_h - 100.0).abs() < 0.5, "{short_h}");
    }
}
