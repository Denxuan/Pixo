use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use gif::{DisposalMethod, Encoder, Frame as GifFrame, Repeat};
use image::RgbaImage;
use tauri::{AppHandle, Emitter, Manager};

use super::clock::unique_file;
use super::error::ScreenshotError;
use super::session::ScreenshotState;
use super::windows;

const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 60);
const MAX_EDGE: u32 = 1080;
const HUD_WIDTH: f64 = 168.0;
const HUD_HEIGHT: f64 = 40.0;
const HUD_PAD: f64 = 8.0;
const FRAME_BORDER: f64 = 2.0;
const PREVIEW_MIN_WIDTH: f64 = 96.0;
const PREVIEW_MIN_HEIGHT: f64 = 64.0;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RecordFormat {
    Gif,
    Mp4,
}

impl RecordFormat {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "gif" => Some(Self::Gif),
            "mp4" => Some(Self::Mp4),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gif => "gif",
            Self::Mp4 => "mp4",
        }
    }

    fn extension(self) -> &'static str {
        self.as_str()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CursorMark {
    Off,
    Highlight,
    Click,
    Both,
}

impl CursorMark {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "highlight" => Some(Self::Highlight),
            "click" => Some(Self::Click),
            "both" => Some(Self::Both),
            _ => None,
        }
    }
}

struct Held {
    path: PathBuf,
    format: RecordFormat,
    save_dir: PathBuf,
    published: Option<PathBuf>,
}

static HELD: Mutex<Option<Held>> = Mutex::new(None);

pub fn is_active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

pub fn stop() {
    STOP.store(true, Ordering::SeqCst);
}

pub struct RecordPlan {
    pub monitor_index: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub monitor_width: u32,
    pub monitor_height: u32,
    pub save_dir: String,
    pub format: RecordFormat,
    pub cursor: CursorMark,
    pub speaker: bool,
    pub microphone: bool,
    pub speaker_device: String,
    pub microphone_device: String,
}

pub fn start(app: AppHandle, plan: RecordPlan) -> Result<(), ScreenshotError> {
    if ACTIVE.swap(true, Ordering::SeqCst) {
        return Err(ScreenshotError::new("busy"));
    }
    discard_held();
    STOP.store(false, Ordering::SeqCst);
    let (left, top) = recorder_origin(
        plan.monitor_x,
        plan.monitor_y,
        plan.monitor_width,
        plan.monitor_height,
        f64::from(plan.x),
        f64::from(plan.y),
        f64::from(plan.width),
        f64::from(plan.height),
        plan.scale,
        HUD_WIDTH,
        HUD_HEIGHT,
    );
    let edges = selection_frame(
        plan.monitor_x,
        plan.monitor_y,
        plan.monitor_width,
        plan.monitor_height,
        f64::from(plan.x),
        f64::from(plan.y),
        f64::from(plan.width),
        f64::from(plan.height),
        plan.scale,
    );
    if let Err(error) = windows::open_record_frame(&app, edges) {
        ACTIVE.store(false, Ordering::SeqCst);
        return Err(error);
    }
    if let Err(error) = windows::open_recorder(
        &app,
        left - HUD_PAD,
        top - HUD_PAD,
        HUD_WIDTH + HUD_PAD * 2.0,
        HUD_HEIGHT + HUD_PAD * 2.0,
    ) {
        windows::close_record_frame(&app);
        ACTIVE.store(false, Ordering::SeqCst);
        return Err(error);
    }
    windows::conceal_overlays(&app);
    let _ = app.emit(
        "screenshot://record",
        serde_json::json!({ "phase": "recording", "format": plan.format.as_str() }),
    );
    thread::spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            capture_and_save(&app, &plan)
        }));
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(_) => {
                eprintln!("screenshot record: panicked");
                RecordOutcome::Failed {
                    code: "capture_failed",
                    reason: None,
                }
            }
        };
        match outcome {
            RecordOutcome::Saved { path, format } => {
                remember_held(&path, format, &plan.save_dir);
                let frame = preview_rect(&plan);
                windows::close_camera(&app);
                windows::close_record_frame(&app);
                let _ = windows::show_record_preview(&app, frame.0, frame.1, frame.2, frame.3);
                let _ = app.emit(
                    "screenshot://record",
                    serde_json::json!({ "phase": "saved", "format": format.as_str() }),
                );
                if let Some(state) = app.try_state::<ScreenshotState>() {
                    let _ = state.finish();
                }
                // Leaving the screenshot session deactivates the app. Put the preview back.
                let _ = windows::show_record_preview(&app, frame.0, frame.1, frame.2, frame.3);
            }
            failed => {
                emit_outcome(&app, &failed);
                thread::sleep(Duration::from_millis(1600));
                windows::close_camera(&app);
                windows::close_record_frame(&app);
                windows::close_recorder(&app);
                if let Some(state) = app.try_state::<ScreenshotState>() {
                    let _ = state.finish();
                }
            }
        }
        ACTIVE.store(false, Ordering::SeqCst);
    });
    Ok(())
}

enum RecordOutcome {
    Saved {
        path: String,
        format: RecordFormat,
    },
    Failed {
        code: &'static str,
        reason: Option<&'static str>,
    },
}

pub fn record_work_dir() -> PathBuf {
    std::env::temp_dir().join("pixo-recording")
}

/// Working media stays outside `save_dir` until the user downloads it.
pub fn scratch_dir(save_dir: &str) -> PathBuf {
    let save = PathBuf::from(save_dir);
    let candidates = [
        record_work_dir(),
        std::env::temp_dir().join("pixo-recording-scratch"),
    ];
    for candidate in candidates {
        if save.as_os_str().is_empty() || !candidate.starts_with(&save) {
            return candidate;
        }
    }
    std::env::temp_dir().join(format!("pixo-recording-{}", std::process::id()))
}

pub fn copy_into_directory(
    source: &Path,
    directory: &Path,
    extension: &str,
) -> Result<PathBuf, String> {
    if !source.is_file() {
        return Err("missing".to_string());
    }
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let dest = unique_file(directory, extension);
    fs::copy(source, &dest).map_err(|error| error.to_string())?;
    Ok(dest)
}

pub fn preview_bytes() -> Result<Vec<u8>, ScreenshotError> {
    let slot = held_lock();
    let held = slot
        .as_ref()
        .ok_or_else(|| ScreenshotError::new("not_found"))?;
    fs::read(&held.path).map_err(|error| {
        eprintln!("record preview: {error}");
        ScreenshotError::new("not_found")
    })
}

pub fn download_preview() -> Result<String, ScreenshotError> {
    let mut slot = held_lock();
    let held = slot
        .as_mut()
        .ok_or_else(|| ScreenshotError::new("not_found"))?;
    if !held.path.is_file() {
        return Err(ScreenshotError::new("not_found"));
    }
    if let Some(path) = &held.published {
        if path.is_file() {
            return Ok(path.to_string_lossy().into_owned());
        }
    }
    if held.save_dir.as_os_str().is_empty() {
        return Err(ScreenshotError::new("storage"));
    }
    let dest = copy_into_directory(&held.path, &held.save_dir, held.format.extension()).map_err(
        |error| {
            eprintln!("record download: {error}");
            ScreenshotError::new("storage")
        },
    )?;
    let path = dest.to_string_lossy().into_owned();
    held.published = Some(dest);
    Ok(path)
}

pub fn copy_preview(app: &AppHandle) -> Result<(), ScreenshotError> {
    let (source, format) = {
        let slot = held_lock();
        let held = slot
            .as_ref()
            .ok_or_else(|| ScreenshotError::new("not_found"))?;
        if !held.path.is_file() {
            return Err(ScreenshotError::new("not_found"));
        }
        (held.path.clone(), held.format)
    };
    let clipboard = app
        .try_state::<crate::clipboard::ClipboardState>()
        .ok_or_else(|| ScreenshotError::new("storage"))?;
    match format {
        RecordFormat::Gif => {
            let bytes = fs::read(&source).map_err(|error| {
                eprintln!("record copy: {error}");
                ScreenshotError::new("storage")
            })?;
            clipboard.copy_gif_image(&bytes).map_err(clipboard_error)
        }
        RecordFormat::Mp4 => {
            let dest = copy_into_directory(&source, &share_dir(app)?, format.extension()).map_err(
                |error| {
                    eprintln!("record copy: {error}");
                    ScreenshotError::new("storage")
                },
            )?;
            clipboard
                .copy_file_path(&dest.to_string_lossy())
                .map_err(clipboard_error)
        }
    }
}

fn clipboard_error(error: crate::clipboard::ClipboardError) -> ScreenshotError {
    eprintln!("record copy: {}", error.code);
    match error.code {
        "file_missing" => ScreenshotError::new("not_found"),
        "unsupported" => ScreenshotError::new("unsupported"),
        "invalid_input" => ScreenshotError::new("invalid_input"),
        "busy" => ScreenshotError::new("busy"),
        "permission_denied" => ScreenshotError::new("permission_denied"),
        _ => ScreenshotError::new("storage"),
    }
}

pub fn close_preview(app: &AppHandle) {
    discard_held();
    windows::close_recorder(app);
}

fn held_lock() -> std::sync::MutexGuard<'static, Option<Held>> {
    HELD.lock().unwrap_or_else(|error| error.into_inner())
}

fn remember_held(path: &str, format: RecordFormat, save_dir: &str) {
    discard_held();
    *held_lock() = Some(Held {
        path: PathBuf::from(path),
        format,
        save_dir: PathBuf::from(save_dir),
        published: None,
    });
}

fn discard_held() {
    let held = held_lock().take();
    if let Some(held) = held {
        let _ = fs::remove_file(held.path);
    }
}

fn share_dir(app: &AppHandle) -> Result<PathBuf, ScreenshotError> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| {
            eprintln!("record share: {error}");
            ScreenshotError::new("storage")
        })?
        .join("screenshot")
        .join("record-shares");
    fs::create_dir_all(&directory).map_err(|error| {
        eprintln!("record share: {error}");
        ScreenshotError::new("storage")
    })?;
    Ok(directory)
}

fn sweep_dir(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.path().is_file() {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn preview_rect(plan: &RecordPlan) -> (f64, f64, f64, f64) {
    preview_window(
        plan.monitor_x,
        plan.monitor_y,
        plan.monitor_width,
        plan.monitor_height,
        f64::from(plan.x),
        f64::from(plan.y),
        f64::from(plan.width),
        f64::from(plan.height),
        plan.scale,
        PREVIEW_MIN_WIDTH,
        PREVIEW_MIN_HEIGHT,
    )
}

fn emit_outcome(app: &AppHandle, outcome: &RecordOutcome) {
    let payload = match outcome {
        RecordOutcome::Saved { path: _, format } => {
            serde_json::json!({ "phase": "saved", "format": format.as_str() })
        }
        RecordOutcome::Failed { code, reason } => {
            let mut params = serde_json::Map::new();
            if let Some(reason) = reason {
                params.insert("reason".to_string(), serde_json::json!(reason));
            }
            serde_json::json!({ "phase": "error", "code": code, "params": params })
        }
    };
    let _ = app.emit("screenshot://record", payload);
}

enum RecordStream {
    Gif(GifStream<BufWriter<File>>),
    Mp4(super::mp4::Mp4Stream),
}

impl RecordStream {
    fn create(format: RecordFormat, directory: &Path, image: &RgbaImage) -> Result<Self, String> {
        match format {
            RecordFormat::Gif => Ok(Self::Gif(GifStream::create_file(directory, image)?)),
            RecordFormat::Mp4 => Ok(Self::Mp4(super::mp4::Mp4Stream::create(image)?)),
        }
    }

    fn push(&mut self, image: &RgbaImage, elapsed: Duration) -> Result<(), String> {
        match self {
            Self::Gif(stream) => stream.push(image, elapsed),
            Self::Mp4(stream) => stream.push(image, elapsed),
        }
    }

    fn finish(
        self,
        directory: &Path,
        tail: Duration,
        audio: Option<Vec<i16>>,
    ) -> Result<String, String> {
        match self {
            Self::Gif(stream) => stream.finish_file(tail),
            Self::Mp4(stream) => stream.finish_file(directory, tail, audio),
        }
    }
}

fn stream_failure(format: RecordFormat, error: &str) -> RecordOutcome {
    eprintln!("screenshot record: {error}");
    if error == "empty" {
        return RecordOutcome::Failed {
            code: "capture_failed",
            reason: Some("empty"),
        };
    }
    if format == RecordFormat::Gif || error == "storage" {
        return RecordOutcome::Failed {
            code: "storage",
            reason: None,
        };
    }
    RecordOutcome::Failed {
        code: "capture_failed",
        reason: None,
    }
}

fn capture_and_save(app: &AppHandle, plan: &RecordPlan) -> RecordOutcome {
    let (x, y, width, height) = match capture_bounds(plan) {
        Ok(bounds) => bounds,
        Err(outcome) => return outcome,
    };
    let directory = scratch_dir(&plan.save_dir);
    if let Err(error) = fs::create_dir_all(&directory) {
        eprintln!("screenshot record: {error}");
        return RecordOutcome::Failed {
            code: "storage",
            reason: None,
        };
    }
    sweep_dir(&directory);
    thread::sleep(Duration::from_millis(80));
    let audio = if plan.format == RecordFormat::Mp4 && (plan.speaker || plan.microphone) {
        match super::audio::AudioCapture::start(
            plan.speaker,
            plan.microphone,
            &plan.speaker_device,
            &plan.microphone_device,
        ) {
            Ok(capture) => Some(capture),
            Err(error) => {
                eprintln!("screenshot record audio: {error}");
                return RecordOutcome::Failed {
                    code: "capture_failed",
                    reason: Some("audio"),
                };
            }
        }
    } else {
        None
    };
    let mut probe = PointerProbe::new();
    let mut pointer_fx = PointerFx::default();
    let mut misses = 0usize;
    let mut captured = 0usize;
    let mut previous_tick = Instant::now();
    let mut stream: Option<RecordStream> = None;
    while !STOP.load(Ordering::SeqCst) {
        let started = Instant::now();
        match capture_region(plan.monitor_index, x, y, width, height) {
            Ok(image) => {
                let mut image = image;
                if plan.cursor != CursorMark::Off {
                    paint_pointer(&mut image, plan, &mut probe, &mut pointer_fx);
                }
                let image = downscale(image);
                let pushed = if let Some(stream) = stream.as_mut() {
                    let elapsed = started.saturating_duration_since(previous_tick);
                    stream.push(&image, elapsed)
                } else {
                    match RecordStream::create(plan.format, &directory, &image) {
                        Ok(created) => {
                            stream = Some(created);
                            Ok(())
                        }
                        Err(error) => Err(error),
                    }
                };
                previous_tick = started;
                if let Err(error) = pushed {
                    stream.take();
                    return stream_failure(plan.format, &error);
                }
                captured += 1;
                misses = 0;
                let spent = started.elapsed();
                if spent < FRAME {
                    thread::sleep(FRAME - spent);
                }
            }
            Err(error) => {
                // One failed grab used to end the recording and drop the rest.
                eprintln!("screenshot record: {error}");
                misses += 1;
                if captured == 0 && misses >= 3 {
                    stream.take();
                    return RecordOutcome::Failed {
                        code: "capture_failed",
                        reason: None,
                    };
                }
                thread::sleep(FRAME);
            }
        }
    }
    let Some(stream) = stream else {
        return RecordOutcome::Failed {
            code: "capture_failed",
            reason: Some("empty"),
        };
    };
    let _ = app.emit(
        "screenshot://record",
        serde_json::json!({ "phase": "exporting", "format": plan.format.as_str() }),
    );
    let audio = audio.map(super::audio::AudioCapture::finish);
    match stream.finish(&directory, previous_tick.elapsed(), audio) {
        Ok(path) => RecordOutcome::Saved {
            path,
            format: plan.format,
        },
        Err(error) => stream_failure(plan.format, &error),
    }
}

fn capture_bounds(plan: &RecordPlan) -> Result<(u32, u32, u32, u32), RecordOutcome> {
    let monitors = match xcap::Monitor::all() {
        Ok(monitors) => monitors,
        Err(error) => {
            eprintln!("screenshot record: {error}");
            return Err(RecordOutcome::Failed {
                code: "capture_failed",
                reason: None,
            });
        }
    };
    let Some(monitor) = monitors.into_iter().nth(plan.monitor_index) else {
        return Err(RecordOutcome::Failed {
            code: "capture_failed",
            reason: Some("display_changed"),
        });
    };
    let bound_w = monitor.width().unwrap_or(0);
    let bound_h = monitor.height().unwrap_or(0);
    let logical = capture_uses_logical_region();
    let scale = plan.scale.max(1.0);
    let expected = |pixels: u32| {
        if logical {
            (f64::from(pixels) / scale).round() as u32
        } else {
            pixels
        }
    };
    if expected(plan.monitor_width).abs_diff(bound_w) > 2
        || expected(plan.monitor_height).abs_diff(bound_h) > 2
    {
        return Err(RecordOutcome::Failed {
            code: "capture_failed",
            reason: Some("display_changed"),
        });
    }
    let (x, y, width, height) =
        region_in_capture_space(plan.x, plan.y, plan.width, plan.height, plan.scale, logical);
    clamp_region(x, y, width, height, bound_w, bound_h).ok_or(RecordOutcome::Failed {
        code: "invalid_input",
        reason: None,
    })
}

fn capture_region(
    index: usize,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<RgbaImage, String> {
    let monitors = xcap::Monitor::all().map_err(|error| error.to_string())?;
    let monitor = monitors
        .into_iter()
        .nth(index)
        .ok_or_else(|| "monitor missing".to_string())?;
    monitor
        .capture_region(x, y, width, height)
        .map_err(|error| error.to_string())
}

fn downscale(image: RgbaImage) -> RgbaImage {
    downscale_limited(image, MAX_EDGE)
}

fn downscale_limited(image: RgbaImage, max_edge: u32) -> RgbaImage {
    let (target_w, target_h) = gif_size(image.width(), image.height(), max_edge);
    if target_w == image.width() && target_h == image.height() {
        return image;
    }
    // Average whole source blocks. A point sample looks jagged, and a triangle
    // resize of a retina frame is slow enough to miss the 60fps budget.
    let step = image
        .width()
        .div_ceil(target_w)
        .max(image.height().div_ceil(target_h))
        .max(1);
    if step <= 1 {
        return image::imageops::resize(
            &image,
            target_w,
            target_h,
            image::imageops::FilterType::Triangle,
        );
    }
    let reduced = box_step(&image, step);
    if reduced.width().max(reduced.height()) <= max_edge {
        return reduced;
    }
    let (width, height) = gif_size(reduced.width(), reduced.height(), max_edge);
    if width == reduced.width() && height == reduced.height() {
        reduced
    } else {
        image::imageops::resize(
            &reduced,
            width,
            height,
            image::imageops::FilterType::Triangle,
        )
    }
}

fn box_step(image: &RgbaImage, step: u32) -> RgbaImage {
    let step = step.max(1) as usize;
    let src_w = image.width() as usize;
    let src_h = image.height() as usize;
    let width = (src_w / step).max(1);
    let height = (src_h / step).max(1);
    let src = image.as_raw();
    let mut out = vec![0u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let x0 = x * step;
            let y0 = y * step;
            let x1 = (x0 + step).min(src_w);
            let y1 = (y0 + step).min(src_h);
            let mut red = 0u32;
            let mut green = 0u32;
            let mut blue = 0u32;
            let mut alpha = 0u32;
            let mut count = 0u32;
            for sy in y0..y1 {
                let row = sy * src_w;
                for sx in x0..x1 {
                    let index = (row + sx) * 4;
                    red += u32::from(src[index]);
                    green += u32::from(src[index + 1]);
                    blue += u32::from(src[index + 2]);
                    alpha += u32::from(src[index + 3]);
                    count += 1;
                }
            }
            let count = count.max(1);
            let offset = (y * width + x) * 4;
            out[offset] = (red / count) as u8;
            out[offset + 1] = (green / count) as u8;
            out[offset + 2] = (blue / count) as u8;
            out[offset + 3] = (alpha / count) as u8;
        }
    }
    RgbaImage::from_raw(width as u32, height as u32, out)
        .unwrap_or_else(|| RgbaImage::new(width as u32, height as u32))
}

pub fn capture_uses_logical_region() -> bool {
    !cfg!(target_os = "windows")
}

pub fn region_in_capture_space(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    scale: f64,
    logical: bool,
) -> (u32, u32, u32, u32) {
    if !logical {
        return (x, y, width.max(1), height.max(1));
    }
    let scale = scale.max(1.0);
    let origin = |value: u32| (f64::from(value) / scale).round().max(0.0) as u32;
    let length = |value: u32| ((f64::from(value) / scale).round() as u32).max(1);
    (origin(x), origin(y), length(width), length(height))
}

pub fn clamp_region(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    bound_w: u32,
    bound_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    if bound_w < 2 || bound_h < 2 || x >= bound_w || y >= bound_h {
        return None;
    }
    let width = width.min(bound_w - x);
    let height = height.min(bound_h - y);
    if width < 2 || height < 2 {
        return None;
    }
    Some((x, y, width, height))
}

pub fn gif_size(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let width = width.max(1);
    let height = height.max(1);
    let edge = width.max(height);
    if max_edge == 0 || edge <= max_edge {
        return (width, height);
    }
    let scale = f64::from(max_edge) / f64::from(edge);
    let next_width = (f64::from(width) * scale).round().max(1.0) as u32;
    let next_height = (f64::from(height) * scale).round().max(1.0) as u32;
    (next_width, next_height)
}

pub fn recorder_origin(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    selection_x: f64,
    selection_y: f64,
    selection_width: f64,
    selection_height: f64,
    scale: f64,
    hud_width: f64,
    hud_height: f64,
) -> (f64, f64) {
    let scale = scale.max(1.0);
    let monitor_left = f64::from(monitor_x) / scale;
    let monitor_top = f64::from(monitor_y) / scale;
    let monitor_right = monitor_left + f64::from(monitor_width) / scale;
    let monitor_bottom = monitor_top + f64::from(monitor_height) / scale;
    let sel_x = monitor_left + selection_x / scale;
    let sel_y = monitor_top + selection_y / scale;
    let sel_w = selection_width / scale;
    let sel_h = selection_height / scale;
    let gap = 12.0;
    let max_x = (monitor_right - hud_width).max(monitor_left);
    let x = (sel_x + sel_w / 2.0 - hud_width / 2.0).clamp(monitor_left, max_x);
    let below = sel_y + sel_h + gap;
    let above = sel_y - gap - hud_height;
    let max_y = (monitor_bottom - hud_height).max(monitor_top);
    let y = if below + hud_height <= monitor_bottom {
        below
    } else if above >= monitor_top {
        above
    } else {
        (sel_y + sel_h - hud_height).clamp(monitor_top, max_y)
    };
    (x, y)
}

/// Four edges around the selection, in logical points: top, right, bottom, left.
/// Each edge sits just outside the captured region so the outline stays visible
/// without covering the picture being recorded. An edge that would leave the
/// monitor is drawn along the inside of that side instead.
pub fn selection_frame(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    selection_x: f64,
    selection_y: f64,
    selection_width: f64,
    selection_height: f64,
    scale: f64,
) -> [Option<(f64, f64, f64, f64)>; 4] {
    let scale = scale.max(1.0);
    let border = FRAME_BORDER;
    let monitor_left = f64::from(monitor_x) / scale;
    let monitor_top = f64::from(monitor_y) / scale;
    let monitor_right = monitor_left + f64::from(monitor_width) / scale;
    let monitor_bottom = monitor_top + f64::from(monitor_height) / scale;
    let left = (monitor_left + selection_x / scale).clamp(monitor_left, monitor_right);
    let top = (monitor_top + selection_y / scale).clamp(monitor_top, monitor_bottom);
    let right = (monitor_left + (selection_x + selection_width) / scale).clamp(left, monitor_right);
    let bottom =
        (monitor_top + (selection_y + selection_height) / scale).clamp(top, monitor_bottom);
    let width = right - left;
    let height = bottom - top;
    if width < 2.0 || height < 2.0 {
        return [None, None, None, None];
    }
    let extend_left = left - border >= monitor_left;
    let extend_right = right + border <= monitor_right;
    let bar_x = if extend_left { left - border } else { left };
    let bar_width = (if extend_right { right + border } else { right }) - bar_x;
    let top_edge = if top - border >= monitor_top {
        band(bar_x, top - border, bar_width, border)
    } else {
        band(left, top, width, border.min(height))
    };
    let right_edge = if right + border <= monitor_right {
        band(right, top, border, height)
    } else {
        band((right - border).max(left), top, border.min(width), height)
    };
    let bottom_edge = if bottom + border <= monitor_bottom {
        band(bar_x, bottom, bar_width, border)
    } else {
        band(left, (bottom - border).max(top), width, border.min(height))
    };
    let left_edge = if left - border >= monitor_left {
        band(left - border, top, border, height)
    } else {
        band(left, top, border.min(width), height)
    };
    [top_edge, right_edge, bottom_edge, left_edge]
}

pub fn preview_window(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    selection_x: f64,
    selection_y: f64,
    selection_width: f64,
    selection_height: f64,
    scale: f64,
    min_width: f64,
    min_height: f64,
) -> (f64, f64, f64, f64) {
    let scale = scale.max(1.0);
    let monitor_left = f64::from(monitor_x) / scale;
    let monitor_top = f64::from(monitor_y) / scale;
    let monitor_right = monitor_left + f64::from(monitor_width) / scale;
    let monitor_bottom = monitor_top + f64::from(monitor_height) / scale;
    let mut left = (monitor_left + selection_x / scale).clamp(monitor_left, monitor_right);
    let mut top = (monitor_top + selection_y / scale).clamp(monitor_top, monitor_bottom);
    let mut width = (selection_width / scale).max(min_width).max(1.0);
    let mut height = (selection_height / scale).max(min_height).max(1.0);
    if left + width > monitor_right {
        width = (monitor_right - left).max(1.0);
    }
    if top + height > monitor_bottom {
        height = (monitor_bottom - top).max(1.0);
    }
    if left + width > monitor_right {
        left = (monitor_right - width).max(monitor_left);
    }
    if top + height > monitor_bottom {
        top = (monitor_bottom - height).max(monitor_top);
    }
    (left, top, width, height)
}

/// Square camera preview at the bottom-right of the selection, in logical points.
pub fn camera_bubble(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    selection_x: f64,
    selection_y: f64,
    selection_width: f64,
    selection_height: f64,
    scale: f64,
) -> Option<(f64, f64, f64, f64)> {
    let scale = scale.max(1.0);
    let monitor_left = f64::from(monitor_x) / scale;
    let monitor_top = f64::from(monitor_y) / scale;
    let monitor_right = monitor_left + f64::from(monitor_width) / scale;
    let monitor_bottom = monitor_top + f64::from(monitor_height) / scale;
    let left = monitor_left + selection_x / scale;
    let top = monitor_top + selection_y / scale;
    let right = (left + selection_width / scale).clamp(left, monitor_right);
    let bottom = (top + selection_height / scale).clamp(top, monitor_bottom);
    let margin = 16.0;
    let room = (right - left - margin * 2.0).min(bottom - top - margin * 2.0);
    let side = 168.0_f64.min(room);
    if side < 72.0 {
        return None;
    }
    let x = (right - margin - side).clamp(monitor_left, (monitor_right - side).max(monitor_left));
    let y = (bottom - margin - side).clamp(monitor_top, (monitor_bottom - side).max(monitor_top));
    Some((x, y, side, side))
}

fn band(x: f64, y: f64, width: f64, height: f64) -> Option<(f64, f64, f64, f64)> {
    if width < 1.0 || height < 1.0 {
        None
    } else {
        Some((x, y, width, height))
    }
}

const COLOR_BITS: u32 = 6;
const COLOR_SHIFT: u32 = 8 - COLOR_BITS;
const COLOR_BINS: usize = 1 << (COLOR_BITS * 3);

struct Hist {
    count: Vec<u32>,
    sum_r: Vec<u32>,
    sum_g: Vec<u32>,
    sum_b: Vec<u32>,
    touched: Vec<u32>,
}

impl Hist {
    fn new() -> Self {
        Self {
            count: vec![0; COLOR_BINS],
            sum_r: vec![0; COLOR_BINS],
            sum_g: vec![0; COLOR_BINS],
            sum_b: vec![0; COLOR_BINS],
            touched: Vec::new(),
        }
    }

    fn add(&mut self, red: u8, green: u8, blue: u8) {
        let bin = color_bin(red, green, blue);
        if self.count[bin] == 0 {
            self.touched.push(bin as u32);
        }
        self.count[bin] += 1;
        self.sum_r[bin] += u32::from(red);
        self.sum_g[bin] += u32::from(green);
        self.sum_b[bin] += u32::from(blue);
    }

    fn clear(&mut self) {
        for bin in self.touched.drain(..) {
            let bin = bin as usize;
            self.count[bin] = 0;
            self.sum_r[bin] = 0;
            self.sum_g[bin] = 0;
            self.sum_b[bin] = 0;
        }
    }

    fn mean(&self, bin: usize) -> [u8; 3] {
        let count = self.count[bin].max(1);
        [
            (self.sum_r[bin] / count) as u8,
            (self.sum_g[bin] / count) as u8,
            (self.sum_b[bin] / count) as u8,
        ]
    }
}

fn color_bin(red: u8, green: u8, blue: u8) -> usize {
    ((usize::from(red) >> COLOR_SHIFT) << (COLOR_BITS * 2))
        | ((usize::from(green) >> COLOR_SHIFT) << COLOR_BITS)
        | (usize::from(blue) >> COLOR_SHIFT)
}

#[cfg(test)]
fn encode_gif(frames: Vec<(RgbaImage, Duration)>) -> Result<Vec<u8>, String> {
    let Some((first, _)) = frames.first() else {
        return Err("empty".to_string());
    };
    let mut stream = GifStream::create(Vec::new(), first)?;
    for pair in frames.windows(2) {
        stream.push(&pair[1].0, pair[0].1)?;
    }
    let tail = frames.last().map(|(_, delay)| *delay).unwrap_or(FRAME);
    let (bytes, _) = stream.finish(tail)?;
    Ok(bytes)
}

struct GifStream<W: Write> {
    encoder: Option<Encoder<W>>,
    width: u32,
    height: u32,
    previous: Vec<u8>,
    pending: Vec<u8>,
    pending_shown: Duration,
    carry_us: i32,
    path: Option<PathBuf>,
    hist: Hist,
}

impl GifStream<BufWriter<File>> {
    fn create_file(directory: &Path, image: &RgbaImage) -> Result<Self, String> {
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let path = unique_file(directory, "gif");
        let file = File::create(&path).map_err(|error| {
            let _ = fs::remove_file(&path);
            error.to_string()
        })?;
        match GifStream::create(BufWriter::new(file), image) {
            Ok(mut stream) => {
                stream.path = Some(path);
                Ok(stream)
            }
            Err(error) => {
                let _ = fs::remove_file(path);
                Err(error)
            }
        }
    }

    fn finish_file(self, tail: Duration) -> Result<String, String> {
        let (mut writer, path) = self.finish(tail)?;
        let Some(path) = path else {
            return Err("gif".to_string());
        };
        if let Err(error) = writer.flush() {
            let _ = fs::remove_file(&path);
            return Err(error.to_string());
        }
        Ok(path.to_string_lossy().into_owned())
    }
}

impl<W: Write> Drop for GifStream<W> {
    fn drop(&mut self) {
        self.pending.clear();
        self.encoder.take();
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

impl<W: Write> GifStream<W> {
    fn create(writer: W, image: &RgbaImage) -> Result<Self, String> {
        let width = u16::try_from(image.width()).map_err(|_| "gif".to_string())?;
        let height = u16::try_from(image.height()).map_err(|_| "gif".to_string())?;
        let mut encoder =
            Encoder::new(writer, width, height, &[]).map_err(|error| error.to_string())?;
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            encoder: Some(encoder),
            width: u32::from(width),
            height: u32::from(height),
            previous: Vec::new(),
            pending: image.as_raw().to_vec(),
            pending_shown: Duration::ZERO,
            carry_us: 0,
            path: None,
            hist: Hist::new(),
        })
    }

    fn push(&mut self, image: &RgbaImage, elapsed: Duration) -> Result<(), String> {
        self.pending_shown += elapsed;
        if image.width() != self.width || image.height() != self.height {
            return Ok(());
        }
        if image.as_raw() == self.pending.as_slice() {
            return Ok(());
        }
        self.emit()?;
        self.pending.clear();
        self.pending.extend_from_slice(image.as_raw());
        self.pending_shown = Duration::ZERO;
        Ok(())
    }

    fn finish(mut self, tail: Duration) -> Result<(W, Option<PathBuf>), String> {
        self.pending_shown += tail;
        self.emit()?;
        let path = self.path.take();
        let Some(encoder) = self.encoder.take() else {
            if let Some(path) = path {
                let _ = fs::remove_file(path);
            }
            return Err("gif".to_string());
        };
        match encoder.into_inner() {
            Ok(writer) => Ok((writer, path)),
            Err(error) => {
                if let Some(path) = path {
                    let _ = fs::remove_file(path);
                }
                Err(error.to_string())
            }
        }
    }

    fn emit(&mut self) -> Result<(), String> {
        let Some(rect) = dirty_rect(&self.previous, &self.pending, self.width, self.height, 4)
        else {
            return Ok(());
        };
        let indexed = index_rgba(&rect.pixels, &mut self.hist);
        let delay = delay_cs(&mut self.carry_us, self.pending_shown);
        self.pending_shown = Duration::ZERO;
        let frame = GifFrame {
            delay,
            dispose: DisposalMethod::Keep,
            transparent: indexed.transparent,
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
            palette: Some(indexed.palette),
            buffer: Cow::Owned(indexed.indices),
            ..GifFrame::default()
        };
        let encoder = self.encoder.as_mut().ok_or_else(|| "gif".to_string())?;
        encoder
            .write_frame(&frame)
            .map_err(|error| error.to_string())?;
        self.previous.clone_from(&self.pending);
        Ok(())
    }
}

struct DirtyRect {
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    pixels: Vec<u8>,
}

fn dirty_rect(
    canvas: &[u8],
    next: &[u8],
    width: u32,
    height: u32,
    pixel: usize,
) -> Option<DirtyRect> {
    let pixel = pixel.max(1);
    let frame_width = u16::try_from(width).ok()?;
    let frame_height = u16::try_from(height).ok()?;
    let needed = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(pixel)?;
    if next.len() < needed || width == 0 || height == 0 {
        return None;
    }
    if canvas.is_empty() {
        return Some(DirtyRect {
            left: 0,
            top: 0,
            width: frame_width,
            height: frame_height,
            pixels: next[..needed].to_vec(),
        });
    }
    if canvas.len() < needed {
        return None;
    }
    let mut top = height;
    let mut bottom = 0u32;
    let mut left = width;
    let mut right = 0u32;
    for y in 0..height {
        let row = (y * width) as usize * pixel;
        for x in 0..width {
            let at = row + x as usize * pixel;
            if canvas[at..at + pixel] == next[at..at + pixel] {
                continue;
            }
            if y < top {
                top = y;
            }
            if y + 1 > bottom {
                bottom = y + 1;
            }
            if x < left {
                left = x;
            }
            if x + 1 > right {
                right = x + 1;
            }
        }
    }
    if top >= bottom || left >= right {
        return None;
    }
    let rect_width = right - left;
    let mut pixels = Vec::with_capacity((rect_width * (bottom - top)) as usize * pixel);
    for y in top..bottom {
        let start = ((y * width + left) as usize) * pixel;
        pixels.extend_from_slice(&next[start..start + rect_width as usize * pixel]);
    }
    Some(DirtyRect {
        left: left as u16,
        top: top as u16,
        width: rect_width as u16,
        height: (bottom - top) as u16,
        pixels,
    })
}

struct IndexedImage {
    palette: Vec<u8>,
    indices: Vec<u8>,
    transparent: Option<u8>,
}

fn index_rgba(raw: &[u8], hist: &mut Hist) -> IndexedImage {
    if let Some(exact) = try_exact(raw) {
        return exact;
    }
    hist.clear();
    let mut transparent = false;
    for pixel in raw.chunks_exact(4) {
        if pixel[3] == 0 {
            transparent = true;
            continue;
        }
        hist.add(pixel[0], pixel[1], pixel[2]);
    }
    let limit = if transparent { 255 } else { 256 };
    let (colors, lookup) = quantize(hist, limit);
    let offset = u8::from(transparent);
    let mut palette = Vec::with_capacity((colors.len() + usize::from(transparent)) * 3);
    if transparent {
        palette.extend_from_slice(&[0, 0, 0]);
    }
    for color in &colors {
        palette.extend_from_slice(color);
    }
    if palette.is_empty() {
        palette.extend_from_slice(&[0, 0, 0]);
    }
    let mut indices = Vec::with_capacity(raw.len() / 4);
    for pixel in raw.chunks_exact(4) {
        if pixel[3] == 0 {
            indices.push(0);
            continue;
        }
        let slot = lookup[color_bin(pixel[0], pixel[1], pixel[2])];
        indices.push(slot.saturating_add(offset));
    }
    hist.clear();
    IndexedImage {
        palette,
        indices,
        transparent: transparent.then_some(0),
    }
}

fn try_exact(raw: &[u8]) -> Option<IndexedImage> {
    if raw.len() / 4 > 12_288 {
        return None;
    }
    let mut transparent = false;
    let mut map = HashMap::<(u8, u8, u8), u8>::new();
    for pixel in raw.chunks_exact(4) {
        if pixel[3] == 0 {
            transparent = true;
            continue;
        }
        map.entry((pixel[0], pixel[1], pixel[2])).or_insert(0);
    }
    let limit = if transparent { 255 } else { 256 };
    if map.len() > limit {
        return None;
    }
    let mut colors: Vec<(u8, u8, u8)> = map.into_keys().collect();
    colors.sort_unstable();
    let map: HashMap<(u8, u8, u8), u8> = colors
        .iter()
        .enumerate()
        .map(|(index, color)| (*color, index as u8))
        .collect();
    let offset = u8::from(transparent);
    let mut palette = Vec::with_capacity((colors.len() + usize::from(transparent)) * 3);
    if transparent {
        palette.extend_from_slice(&[0, 0, 0]);
    }
    for (red, green, blue) in &colors {
        palette.extend_from_slice(&[*red, *green, *blue]);
    }
    if palette.is_empty() {
        palette.extend_from_slice(&[0, 0, 0]);
    }
    let mut indices = Vec::with_capacity(raw.len() / 4);
    for pixel in raw.chunks_exact(4) {
        if pixel[3] == 0 {
            indices.push(0);
            continue;
        }
        let slot = map
            .get(&(pixel[0], pixel[1], pixel[2]))
            .copied()
            .unwrap_or(0);
        indices.push(slot.saturating_add(offset));
    }
    Some(IndexedImage {
        palette,
        indices,
        transparent: transparent.then_some(0),
    })
}

fn quantize(hist: &Hist, limit: usize) -> (Vec<[u8; 3]>, Vec<u8>) {
    let occupied: Vec<u32> = hist
        .touched
        .iter()
        .copied()
        .filter(|bin| hist.count[*bin as usize] > 0)
        .collect();
    let mut lookup = vec![0u8; COLOR_BINS];
    if occupied.is_empty() {
        return (Vec::new(), lookup);
    }
    let limit = limit.max(1);
    if occupied.len() <= limit {
        let mut colors = Vec::with_capacity(occupied.len());
        for (slot, bin) in occupied.iter().enumerate() {
            lookup[*bin as usize] = slot as u8;
            colors.push(hist.mean(*bin as usize));
        }
        return (colors, lookup);
    }
    let means = bin_means(hist);
    let colors = median_cut(&means, hist, &occupied, limit);
    for bin in occupied {
        lookup[bin as usize] = nearest(means[bin as usize], &colors);
    }
    (colors, lookup)
}

fn bin_means(hist: &Hist) -> Vec<[u8; 3]> {
    let mut means = vec![[0u8; 3]; COLOR_BINS];
    for bin in &hist.touched {
        let bin = *bin as usize;
        if hist.count[bin] > 0 {
            means[bin] = hist.mean(bin);
        }
    }
    means
}

fn median_cut(means: &[[u8; 3]], hist: &Hist, occupied: &[u32], limit: usize) -> Vec<[u8; 3]> {
    let (range, channel) = channel_range(means, occupied);
    let mut cubes = vec![ColorCube {
        bins: occupied.to_vec(),
        range,
        channel,
    }];
    while cubes.len() < limit {
        let mut choice: Option<(usize, i32)> = None;
        for (index, cube) in cubes.iter().enumerate() {
            if cube.bins.len() < 2 || cube.range <= 0 {
                continue;
            }
            let replace = choice.is_none_or(|(_, best)| cube.range > best);
            if replace {
                choice = Some((index, cube.range));
            }
        }
        let Some((index, _)) = choice else {
            break;
        };
        let cube = cubes.swap_remove(index);
        let (left, right) = split_bins(means, hist, cube.bins, cube.channel);
        cubes.push(cube_with_range(means, left));
        cubes.push(cube_with_range(means, right));
    }
    cubes
        .iter()
        .map(|cube| cube_mean(hist, &cube.bins))
        .collect()
}

struct ColorCube {
    bins: Vec<u32>,
    range: i32,
    channel: usize,
}

fn cube_with_range(means: &[[u8; 3]], bins: Vec<u32>) -> ColorCube {
    let (range, channel) = channel_range(means, &bins);
    ColorCube {
        bins,
        range,
        channel,
    }
}

fn channel_range(means: &[[u8; 3]], bins: &[u32]) -> (i32, usize) {
    let mut min = [255i32; 3];
    let mut max = [0i32; 3];
    for bin in bins {
        let color = means[*bin as usize];
        for channel in 0..3 {
            min[channel] = min[channel].min(i32::from(color[channel]));
            max[channel] = max[channel].max(i32::from(color[channel]));
        }
    }
    let mut chosen = 0;
    let mut range = max[0] - min[0];
    for channel in 1..3 {
        let next = max[channel] - min[channel];
        if next > range {
            range = next;
            chosen = channel;
        }
    }
    (range, chosen)
}

fn split_bins(
    means: &[[u8; 3]],
    hist: &Hist,
    mut bins: Vec<u32>,
    channel: usize,
) -> (Vec<u32>, Vec<u32>) {
    bins.sort_by_key(|bin| means[*bin as usize][channel]);
    let total: u64 = bins
        .iter()
        .map(|bin| u64::from(hist.count[*bin as usize]))
        .sum();
    let mut running = 0u64;
    let mut split_at = 1usize;
    for (index, bin) in bins.iter().enumerate() {
        running += u64::from(hist.count[*bin as usize]);
        if running >= total / 2 && index + 1 < bins.len() {
            split_at = index + 1;
            break;
        }
    }
    let right = bins.split_off(split_at.min(bins.len().saturating_sub(1)).max(1));
    (bins, right)
}

fn cube_mean(hist: &Hist, bins: &[u32]) -> [u8; 3] {
    let mut count = 0u64;
    let mut sum = [0u64; 3];
    for bin in bins {
        let bin = *bin as usize;
        let samples = u64::from(hist.count[bin]);
        count += samples;
        sum[0] += u64::from(hist.sum_r[bin]);
        sum[1] += u64::from(hist.sum_g[bin]);
        sum[2] += u64::from(hist.sum_b[bin]);
    }
    if count == 0 {
        return [0, 0, 0];
    }
    [
        (sum[0] / count) as u8,
        (sum[1] / count) as u8,
        (sum[2] / count) as u8,
    ]
}

fn nearest(color: [u8; 3], palette: &[[u8; 3]]) -> u8 {
    let mut best = 0u8;
    let mut best_distance = i32::MAX;
    for (index, candidate) in palette.iter().enumerate() {
        let distance = color_distance(color, *candidate);
        if distance < best_distance {
            best_distance = distance;
            best = index as u8;
            if distance == 0 {
                break;
            }
        }
    }
    best
}

fn color_distance(left: [u8; 3], right: [u8; 3]) -> i32 {
    let red = i32::from(left[0]) - i32::from(right[0]);
    let green = i32::from(left[1]) - i32::from(right[1]);
    let blue = i32::from(left[2]) - i32::from(right[2]);
    red * red + green * green + blue * blue
}

const POINTER_BLUE: [u8; 3] = [59, 130, 246];
const CLICK_FADE_MS: f64 = 450.0;

struct PointerSample {
    x: f64,
    y: f64,
    down: bool,
}

#[derive(Default)]
struct PointerFx {
    was_down: bool,
    click: Option<ClickMark>,
}

struct ClickMark {
    x: f64,
    y: f64,
    at: Instant,
}

struct PointerProbe {
    #[cfg(target_os = "linux")]
    linux: LinuxProbe,
}

impl PointerProbe {
    fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            linux: LinuxProbe::new(),
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn sample(&mut self) -> Option<PointerSample> {
        let _ = self;
        host_pointer()
    }

    #[cfg(target_os = "linux")]
    fn sample(&mut self) -> Option<PointerSample> {
        self.linux.sample()
    }
}

fn paint_pointer(
    image: &mut RgbaImage,
    plan: &RecordPlan,
    probe: &mut PointerProbe,
    fx: &mut PointerFx,
) {
    let highlight = matches!(plan.cursor, CursorMark::Highlight | CursorMark::Both);
    let clicks = matches!(plan.cursor, CursorMark::Click | CursorMark::Both);
    let radius = pointer_radius(image.width(), image.height());
    let width = pointer_width(radius);
    if let Some(sample) = probe.sample() {
        let (x, y) = pointer_in_capture(
            sample.x,
            sample.y,
            plan.monitor_x,
            plan.monitor_y,
            plan.x,
            plan.y,
            plan.scale,
            capture_uses_logical_region(),
        );
        let inside = point_near(x, y, image.width(), image.height());
        if clicks && sample.down && !fx.was_down && inside {
            fx.click = Some(ClickMark {
                x,
                y,
                at: Instant::now(),
            });
        }
        fx.was_down = sample.down;
        if highlight && inside {
            paint_disk(
                image,
                x,
                y,
                radius,
                [POINTER_BLUE[0], POINTER_BLUE[1], POINTER_BLUE[2], 70],
            );
            paint_ring(
                image,
                x,
                y,
                radius,
                width,
                [POINTER_BLUE[0], POINTER_BLUE[1], POINTER_BLUE[2], 235],
            );
        }
    }
    if !clicks {
        return;
    }
    let rendered = fx.click.as_ref().map(|click| {
        let age = click.at.elapsed();
        (
            click.x,
            click.y,
            click_alpha(age),
            click_radius(radius, age),
        )
    });
    let Some((x, y, alpha, click_radius)) = rendered else {
        return;
    };
    if alpha == 0 {
        fx.click = None;
        return;
    }
    paint_ring(image, x, y, click_radius, width, [255, 255, 255, alpha]);
    paint_disk(image, x, y, radius * 0.36, [255, 255, 255, alpha]);
}

fn pointer_radius(width: u32, height: u32) -> f64 {
    let edge = f64::from(width.min(height));
    (edge * 0.045).clamp(18.0, 96.0)
}

fn pointer_width(radius: f64) -> f64 {
    (radius * 0.12).clamp(2.0, 6.0)
}

fn click_radius(base: f64, age: Duration) -> f64 {
    let t = (age.as_secs_f64() / (CLICK_FADE_MS / 1000.0)).clamp(0.0, 1.0);
    base * (0.55 + t * 1.25)
}

fn pointer_in_capture(
    global_x: f64,
    global_y: f64,
    monitor_x: i32,
    monitor_y: i32,
    selection_x: u32,
    selection_y: u32,
    scale: f64,
    logical: bool,
) -> (f64, f64) {
    let scale = if logical { scale.max(1.0) } else { 1.0 };
    (
        global_x - f64::from(monitor_x) / scale - f64::from(selection_x) / scale,
        global_y - f64::from(monitor_y) / scale - f64::from(selection_y) / scale,
    )
}

fn point_near(x: f64, y: f64, width: u32, height: u32) -> bool {
    x.is_finite()
        && y.is_finite()
        && x >= -24.0
        && y >= -24.0
        && x < f64::from(width) + 24.0
        && y < f64::from(height) + 24.0
}

fn click_alpha(age: Duration) -> u8 {
    if age >= Duration::from_millis(CLICK_FADE_MS as u64) {
        return 0;
    }
    let t = (age.as_secs_f64() / (CLICK_FADE_MS / 1000.0)).clamp(0.0, 1.0);
    ((1.0 - t) * 255.0).round() as u8
}

fn paint_ring(
    image: &mut RgbaImage,
    cx: f64,
    cy: f64,
    radius: f64,
    thickness: f64,
    color: [u8; 4],
) {
    let inner = (radius - thickness).max(0.0);
    let reach = radius + 1.0;
    let x0 = (cx - reach).floor() as i32;
    let x1 = (cx + reach).ceil() as i32;
    let y0 = (cy - reach).floor() as i32;
    let y1 = (cy + reach).ceil() as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = f64::from(x) + 0.5 - cx;
            let dy = f64::from(y) + 0.5 - cy;
            let dist = dx.hypot(dy);
            let outer = (radius + 0.75 - dist).clamp(0.0, 1.0);
            let hole = (dist - (inner - 0.75)).clamp(0.0, 1.0);
            let cover = outer.min(hole);
            if cover <= 0.0 {
                continue;
            }
            let alpha = (f64::from(color[3]) * cover).round() as u8;
            if alpha == 0 {
                continue;
            }
            blend_pixel(image, x, y, [color[0], color[1], color[2], alpha]);
        }
    }
}

fn paint_disk(image: &mut RgbaImage, cx: f64, cy: f64, radius: f64, color: [u8; 4]) {
    let reach = radius + 1.0;
    let x0 = (cx - reach).floor() as i32;
    let x1 = (cx + reach).ceil() as i32;
    let y0 = (cy - reach).floor() as i32;
    let y1 = (cy + reach).ceil() as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = f64::from(x) + 0.5 - cx;
            let dy = f64::from(y) + 0.5 - cy;
            let cover = (radius + 0.75 - dx.hypot(dy)).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let alpha = (f64::from(color[3]) * cover).round() as u8;
            if alpha == 0 {
                continue;
            }
            blend_pixel(image, x, y, [color[0], color[1], color[2], alpha]);
        }
    }
}

fn blend_pixel(image: &mut RgbaImage, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 {
        return;
    }
    let (width, height) = (image.width(), image.height());
    if x as u32 >= width || y as u32 >= height || color[3] == 0 {
        return;
    }
    let pixel = image.get_pixel_mut(x as u32, y as u32);
    if color[3] == 255 {
        *pixel = image::Rgba([color[0], color[1], color[2], 255]);
        return;
    }
    let src_a = u32::from(color[3]);
    let inv = 255 - src_a;
    let dst = pixel.0;
    pixel.0 = [
        ((u32::from(color[0]) * src_a + u32::from(dst[0]) * inv) / 255) as u8,
        ((u32::from(color[1]) * src_a + u32::from(dst[1]) * inv) / 255) as u8,
        ((u32::from(color[2]) * src_a + u32::from(dst[2]) * inv) / 255) as u8,
        255,
    ];
}

#[cfg(target_os = "macos")]
fn host_pointer() -> Option<PointerSample> {
    unsafe {
        let event = CGEventCreate(std::ptr::null_mut());
        if event.is_null() {
            return None;
        }
        let point = CGEventGetLocation(event);
        CFRelease(event);
        let down = CGEventSourceButtonState(0, 0) || CGEventSourceButtonState(0, 1);
        Some(PointerSample {
            x: point.x,
            y: point.y,
            down,
        })
    }
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct CgPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreate(source: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CgPoint;
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const std::ffi::c_void);
}

#[cfg(target_os = "windows")]
fn host_pointer() -> Option<PointerSample> {
    // `windows` in this module is the overlay helper. The Win32 crate is at the root.
    use ::windows::Win32::Foundation::POINT;
    use ::windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    use ::windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return None;
    }
    let down = unsafe {
        GetAsyncKeyState(i32::from(VK_LBUTTON.0)) < 0
            || GetAsyncKeyState(i32::from(VK_RBUTTON.0)) < 0
    };
    Some(PointerSample {
        x: f64::from(point.x),
        y: f64::from(point.y),
        down,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn host_pointer() -> Option<PointerSample> {
    None
}

#[cfg(target_os = "linux")]
struct LinuxProbe {
    state: LinuxState,
}

#[cfg(target_os = "linux")]
enum LinuxState {
    Pending,
    Failed,
    Open {
        connection: x11rb::rust_connection::RustConnection,
        root: u32,
    },
}

#[cfg(target_os = "linux")]
impl LinuxProbe {
    fn new() -> Self {
        Self {
            state: LinuxState::Pending,
        }
    }

    fn sample(&mut self) -> Option<PointerSample> {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::{ConnectionExt, KeyButMask};

        if matches!(self.state, LinuxState::Pending) {
            self.state = open_linux_pointer()
                .map(|(connection, root)| LinuxState::Open { connection, root })
                .unwrap_or(LinuxState::Failed);
        }
        let LinuxState::Open { connection, root } = &self.state else {
            return None;
        };
        let reply = connection.query_pointer(*root).ok()?.reply().ok()?;
        let buttons = u16::from(KeyButMask::BUTTON1)
            | u16::from(KeyButMask::BUTTON2)
            | u16::from(KeyButMask::BUTTON3);
        Some(PointerSample {
            x: f64::from(reply.root_x),
            y: f64::from(reply.root_y),
            down: u16::from(reply.mask) & buttons != 0,
        })
    }
}

#[cfg(target_os = "linux")]
fn open_linux_pointer() -> Option<(x11rb::rust_connection::RustConnection, u32)> {
    use x11rb::connection::Connection;

    if super::capture::is_wayland() {
        return None;
    }
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    Some((connection, root))
}

fn delay_cs(carry_us: &mut i32, elapsed: Duration) -> u16 {
    let micros = elapsed.as_micros().min(i32::MAX as u128);
    *carry_us = carry_us.saturating_add(i32::try_from(micros).unwrap_or(i32::MAX));
    let mut centiseconds = *carry_us / 10_000;
    *carry_us %= 10_000;
    if centiseconds < 1 {
        centiseconds = 1;
        *carry_us -= 10_000;
    }
    u16::try_from(centiseconds).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_mark_accepts_the_pointer_effects() {
        assert_eq!(CursorMark::parse("off"), Some(CursorMark::Off));
        assert_eq!(CursorMark::parse("highlight"), Some(CursorMark::Highlight));
        assert_eq!(CursorMark::parse("click"), Some(CursorMark::Click));
        assert_eq!(CursorMark::parse("both"), Some(CursorMark::Both));
        assert_eq!(CursorMark::parse("camera"), None);
    }

    #[test]
    fn pointer_in_capture_converts_scaled_points() {
        let (x, y) = pointer_in_capture(80.0, 45.0, 0, 0, 100, 50, 2.0, true);
        assert!((x - 30.0).abs() < 0.001);
        assert!((y - 20.0).abs() < 0.001);
        let (x, y) = pointer_in_capture(180.0, 90.0, 100, 40, 20, 10, 2.0, false);
        assert!((x - 60.0).abs() < 0.001);
        assert!((y - 40.0).abs() < 0.001);
    }

    #[test]
    fn click_alpha_fades_out_over_450ms() {
        assert_eq!(click_alpha(Duration::ZERO), 255);
        assert_eq!(click_alpha(Duration::from_millis(450)), 0);
        let mid = click_alpha(Duration::from_millis(225));
        assert!((120..=136).contains(&u32::from(mid)));
    }

    #[test]
    fn pointer_radius_grows_with_the_frame() {
        assert!((pointer_radius(200, 200) - 18.0).abs() < 0.01);
        assert!((pointer_radius(1000, 800) - 36.0).abs() < 0.01);
        assert!((pointer_radius(4000, 3000) - 96.0).abs() < 0.01);
    }

    #[test]
    fn click_ring_expands_as_it_fades() {
        let start = click_radius(40.0, Duration::ZERO);
        let later = click_radius(40.0, Duration::from_millis(225));
        assert!((start - 22.0).abs() < 0.01);
        assert!(later > start);
    }

    #[test]
    fn pointer_ring_marks_the_click_and_spares_the_center() {
        let mut image = RgbaImage::from_pixel(48, 48, image::Rgba([0, 0, 0, 255]));
        paint_ring(&mut image, 24.0, 24.0, 18.0, 2.0, [59, 130, 246, 255]);
        assert!(image.get_pixel(41, 24).0[2] > 80);
        assert_eq!(image.get_pixel(24, 24).0, [0, 0, 0, 255]);
    }

    #[test]
    fn logical_region_divides_by_the_display_scale() {
        assert_eq!(
            region_in_capture_space(100, 40, 200, 80, 2.0, true),
            (50, 20, 100, 40)
        );
        assert_eq!(
            region_in_capture_space(100, 40, 200, 80, 2.0, false),
            (100, 40, 200, 80)
        );
    }

    #[test]
    fn region_outside_the_monitor_is_rejected() {
        assert_eq!(
            clamp_region(10, 10, 40, 20, 100, 80),
            Some((10, 10, 40, 20))
        );
        assert_eq!(
            clamp_region(90, 10, 40, 20, 100, 80),
            Some((90, 10, 10, 20))
        );
        assert!(clamp_region(100, 0, 10, 10, 100, 80).is_none());
    }

    #[test]
    fn gif_frame_fits_the_long_edge() {
        assert_eq!(gif_size(3024, 1000, 720), (720, 238));
        assert_eq!(gif_size(100, 80, 720), (100, 80));
    }

    #[test]
    fn downscale_samples_a_retina_frame_down_to_the_long_edge() {
        let image = RgbaImage::from_pixel(8, 4, image::Rgba([9, 8, 7, 255]));
        let scaled = downscale_limited(image, 4);
        assert_eq!(scaled.dimensions(), (4, 2));
        assert_eq!(scaled.get_pixel(0, 0).0, [9, 8, 7, 255]);
    }

    #[test]
    fn downscale_averages_neighboring_pixels() {
        let mut image = RgbaImage::new(2, 2);
        image.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        image.put_pixel(1, 0, image::Rgba([255, 0, 0, 255]));
        image.put_pixel(0, 1, image::Rgba([0, 0, 0, 255]));
        image.put_pixel(1, 1, image::Rgba([0, 0, 0, 255]));
        let scaled = downscale_limited(image, 1);
        assert_eq!(scaled.dimensions(), (1, 1));
        assert_eq!(scaled.get_pixel(0, 0).0, [63, 0, 0, 255]);
    }

    #[test]
    fn selection_frame_outlines_the_region() {
        assert_eq!(
            selection_frame(0, 0, 1512, 982, 100.0, 200.0, 400.0, 300.0, 2.0),
            [
                Some((48.0, 98.0, 204.0, 2.0)),
                Some((250.0, 100.0, 2.0, 150.0)),
                Some((48.0, 250.0, 204.0, 2.0)),
                Some((48.0, 100.0, 2.0, 150.0)),
            ]
        );
    }

    #[test]
    fn camera_bubble_sits_in_the_bottom_right() {
        assert_eq!(
            camera_bubble(0, 0, 2000, 1200, 100.0, 50.0, 800.0, 600.0, 2.0),
            Some((266.0, 141.0, 168.0, 168.0))
        );
        assert_eq!(
            camera_bubble(0, 0, 400, 300, 0.0, 0.0, 80.0, 80.0, 1.0),
            None
        );
    }

    #[test]
    fn selection_frame_stays_on_the_monitor_edge() {
        assert_eq!(
            selection_frame(0, 0, 1512, 982, 0.0, 0.0, 400.0, 300.0, 2.0),
            [
                Some((0.0, 0.0, 200.0, 2.0)),
                Some((200.0, 0.0, 2.0, 150.0)),
                Some((0.0, 150.0, 202.0, 2.0)),
                Some((0.0, 0.0, 2.0, 150.0)),
            ]
        );
    }

    #[test]
    fn preview_window_covers_the_selection() {
        assert_eq!(
            preview_window(0, 0, 1512, 982, 100.0, 200.0, 400.0, 300.0, 2.0, 96.0, 64.0),
            (50.0, 100.0, 200.0, 150.0)
        );
        assert_eq!(
            preview_window(0, 0, 1512, 982, 0.0, 0.0, 20.0, 20.0, 2.0, 96.0, 64.0),
            (0.0, 0.0, 96.0, 64.0)
        );
    }

    #[test]
    fn recorder_bar_uses_the_format_toolbar_slot() {
        assert_eq!(
            recorder_origin(
                0, 0, 1512, 982, 100.0, 200.0, 400.0, 300.0, 2.0, 168.0, 40.0
            ),
            (66.0, 262.0)
        );
    }

    #[test]
    fn recorder_bar_moves_above_when_the_bottom_is_blocked() {
        assert_eq!(
            recorder_origin(0, 0, 1512, 982, 200.0, 920.0, 400.0, 40.0, 2.0, 168.0, 40.0),
            (116.0, 408.0)
        );
    }

    #[test]
    fn recorder_bar_stays_inside_when_the_selection_fills_the_monitor() {
        assert_eq!(
            recorder_origin(0, 0, 1512, 982, 0.0, 0.0, 1512.0, 982.0, 2.0, 168.0, 40.0),
            (294.0, 451.0)
        );
    }

    #[test]
    fn two_frames_encode_as_a_gif() {
        let frame = RgbaImage::from_pixel(2, 2, image::Rgba([220, 40, 40, 255]));
        let bytes = encode_gif(vec![
            (frame.clone(), Duration::from_millis(100)),
            (frame, Duration::from_millis(100)),
        ])
        .unwrap();
        assert!(bytes.starts_with(b"GIF89a"));
    }

    #[test]
    fn quantizer_keeps_solid_colors_and_fits_the_palette() {
        let solid = RgbaImage::from_pixel(4, 4, image::Rgba([220, 30, 40, 255]));
        let indexed = index_rgba(solid.as_raw(), &mut Hist::new());
        assert_eq!(indexed.palette, vec![220, 30, 40]);
        assert!(indexed.indices.iter().all(|index| *index == 0));

        let mut mixed = RgbaImage::new(2, 1);
        mixed.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        mixed.put_pixel(1, 0, image::Rgba([0, 0, 255, 255]));
        let indexed = index_rgba(mixed.as_raw(), &mut Hist::new());
        assert_ne!(indexed.indices[0], indexed.indices[1]);
        let red = indexed.indices[0] as usize * 3;
        let blue = indexed.indices[1] as usize * 3;
        assert!(indexed.palette[red] > 200);
        assert!(indexed.palette[blue + 2] > 200);

        let mut broad = RgbaImage::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                broad.put_pixel(x, y, image::Rgba([x as u8 * 8, y as u8 * 8, 40, 255]));
            }
        }
        let indexed = index_rgba(broad.as_raw(), &mut Hist::new());
        assert!(indexed.palette.len() / 3 <= 256);
        assert!(indexed.palette.len() / 3 > 1);
        assert_eq!(indexed.indices.len(), 32 * 32);
    }

    #[test]
    fn delays_average_sixty_frames_per_second() {
        let mut carry = 0;
        let step = Duration::from_nanos(1_000_000_000 / 60);
        let mut total = 0u32;
        for _ in 0..60 {
            total += u32::from(delay_cs(&mut carry, step));
        }
        assert!((total as i32 - 100).abs() <= 1);
    }

    #[test]
    fn scratch_dir_stays_out_of_the_save_folder() {
        let save = PathBuf::from("/Users/zdx/Pictures/Pixo");
        let scratch = scratch_dir(save.to_str().unwrap());
        assert!(scratch.starts_with(std::env::temp_dir()));
        assert!(!scratch.starts_with(&save));
    }

    #[test]
    fn download_copies_into_the_save_folder_and_close_can_drop_the_work_file() {
        let root = std::env::temp_dir().join(format!("pixo-record-copy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let work = root.join("work");
        let save = root.join("Pictures");
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&save).unwrap();
        let source = work.join("clip.mp4");
        fs::write(&source, b"mp4-bytes").unwrap();
        assert!(fs::read_dir(&save).unwrap().next().is_none());
        let saved = copy_into_directory(&source, &save, "mp4").unwrap();
        assert!(saved.starts_with(&save));
        assert!(!saved.starts_with(&work));
        assert_eq!(fs::read(&source).unwrap(), fs::read(&saved).unwrap());
        fs::remove_file(&source).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(&saved).unwrap(), b"mp4-bytes");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dirty_rect_covers_only_the_changed_pixels() {
        let canvas = vec![0u8; 16];
        let mut next = canvas.clone();
        next[5] = 1;
        let rect = dirty_rect(&canvas, &next, 4, 4, 1).unwrap();
        assert_eq!((rect.left, rect.top, rect.width, rect.height), (1, 1, 1, 1));
        assert_eq!(rect.pixels, vec![1]);
    }
}
