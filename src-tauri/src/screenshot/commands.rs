use tauri::{AppHandle, Manager, State, WebviewWindow};

use super::error::ScreenshotError;
use super::model::{EditorDocument, ShotSettings};
use super::session::{CopyResult, SaveResult, ScreenshotState, SessionView, ShotStatus};

#[tauri::command]
pub fn screenshot_status(state: State<'_, ScreenshotState>) -> ShotStatus {
    state.status()
}

#[tauri::command]
pub fn screenshot_get_settings(state: State<'_, ScreenshotState>) -> ShotSettings {
    state.settings()
}

#[tauri::command]
pub fn screenshot_update_settings(
    state: State<'_, ScreenshotState>,
    settings: ShotSettings,
) -> Result<ShotSettings, ScreenshotError> {
    state.update_settings(settings)
}

#[tauri::command]
pub fn screenshot_set_enabled(
    state: State<'_, ScreenshotState>,
    enabled: bool,
) -> Result<ShotSettings, ScreenshotError> {
    state.set_enabled(enabled)
}

#[tauri::command(async)]
pub async fn screenshot_pick_save_dir(
    state: State<'_, ScreenshotState>,
    title: String,
) -> Result<Option<String>, ScreenshotError> {
    state.pick_save_dir(&title).await
}

#[tauri::command]
pub fn screenshot_request_access(state: State<'_, ScreenshotState>) -> ShotStatus {
    state.request_access()
}

#[tauri::command(async)]
pub async fn screenshot_start(
    state: State<'_, ScreenshotState>,
    mode: Option<String>,
    hide_self: Option<bool>,
) -> Result<SessionView, ScreenshotError> {
    let state = (*state).clone();
    let settings = state.settings();
    let mode = mode.unwrap_or_else(|| "capture".to_string());
    let hide = hide_self.unwrap_or(settings.hide_self);
    tauri::async_runtime::spawn_blocking(move || state.start(&mode, hide))
        .await
        .map_err(|error| {
            eprintln!("screenshot start: {error}");
            ScreenshotError::new("capture_failed")
        })?
}

#[tauri::command]
pub fn screenshot_current(
    state: State<'_, ScreenshotState>,
) -> Result<SessionView, ScreenshotError> {
    state.current()
}

#[tauri::command]
pub fn screenshot_overlay_ready(
    window: WebviewWindow,
    state: State<'_, ScreenshotState>,
    session_id: u64,
) -> Result<(), ScreenshotError> {
    let active = state.current().ok().map(|view| view.id);
    super::windows::reveal_armed(&window, session_id, active);
    Ok(())
}

#[tauri::command(async)]
pub async fn screenshot_frame(
    state: State<'_, ScreenshotState>,
    index: u32,
) -> Result<tauri::ipc::Response, ScreenshotError> {
    let state = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || state.frame(index))
        .await
        .map_err(|error| {
            eprintln!("screenshot frame: {error}");
            ScreenshotError::new("capture_failed")
        })?
        .map(tauri::ipc::Response::new)
}

#[tauri::command]
pub fn screenshot_editor_commit(
    state: State<'_, ScreenshotState>,
    base_version: u64,
    document: EditorDocument,
) -> Result<SessionView, ScreenshotError> {
    state.commit(base_version, document)
}

#[tauri::command]
pub fn screenshot_editor_undo(
    state: State<'_, ScreenshotState>,
) -> Result<SessionView, ScreenshotError> {
    state.undo()
}

#[tauri::command]
pub fn screenshot_editor_redo(
    state: State<'_, ScreenshotState>,
) -> Result<SessionView, ScreenshotError> {
    state.redo()
}

#[tauri::command]
pub fn screenshot_cancel(state: State<'_, ScreenshotState>) -> Result<(), ScreenshotError> {
    state.cancel()
}

#[tauri::command]
pub fn screenshot_finish(state: State<'_, ScreenshotState>) -> Result<(), ScreenshotError> {
    state.finish()
}

#[tauri::command(async)]
pub async fn screenshot_recognize_text(
    png_base64: String,
) -> Result<super::ocr::OcrText, ScreenshotError> {
    tauri::async_runtime::spawn_blocking(move || super::ocr::recognize(&png_base64))
        .await
        .map_err(|error| {
            eprintln!("screenshot ocr: {error}");
            ScreenshotError::new("capture_failed")
        })?
}

#[tauri::command]
pub fn screenshot_copy_text(app: AppHandle, text: String) -> Result<(), ScreenshotError> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 1024 * 1024 {
        return Err(ScreenshotError::new("invalid_input"));
    }
    if let Some(clipboard) = app.try_state::<crate::clipboard::ClipboardState>() {
        clipboard
            .note_text(trimmed, "screenshot")
            .map_err(|error| {
                eprintln!("screenshot ocr clipboard: {}", error.code);
                ScreenshotError::new(match error.code {
                    "busy" => "busy",
                    "too_large" => "too_large",
                    "invalid_input" => "invalid_input",
                    _ => "storage",
                })
            })?;
    } else {
        super::ocr::copy_text(trimmed)?;
    }
    if let Some(state) = app.try_state::<ScreenshotState>() {
        state.note_ocr(trimmed);
    }
    Ok(())
}

#[tauri::command]
pub fn screenshot_library_list(
    state: State<'_, ScreenshotState>,
    query: Option<String>,
) -> Vec<super::library::LibraryItem> {
    state.library_list(query.as_deref())
}

#[tauri::command]
pub fn screenshot_library_delete(
    state: State<'_, ScreenshotState>,
    id: i64,
) -> Result<(), ScreenshotError> {
    state.library_delete(id)
}

#[tauri::command]
pub fn screenshot_library_copy(
    state: State<'_, ScreenshotState>,
    id: i64,
) -> Result<(), ScreenshotError> {
    state.library_copy(id)
}

#[tauri::command]
pub fn screenshot_library_path(
    state: State<'_, ScreenshotState>,
    id: i64,
) -> Result<String, ScreenshotError> {
    state.library_path(id)
}

#[tauri::command]
pub fn screenshot_library_thumb(
    state: State<'_, ScreenshotState>,
    id: i64,
) -> Result<tauri::ipc::Response, ScreenshotError> {
    Ok(tauri::ipc::Response::new(state.library_thumb(id)?))
}

#[tauri::command]
pub fn screenshot_scroll_stop() {
    super::scroll::request_stop();
}

#[tauri::command]
pub fn screenshot_scroll_set_auto(
    state: State<'_, ScreenshotState>,
    enabled: bool,
) -> Result<ShotSettings, ScreenshotError> {
    state.set_scroll_auto(enabled)
}

#[tauri::command(async)]
pub async fn screenshot_scroll(
    window: WebviewWindow,
    state: State<'_, ScreenshotState>,
    monitor_index: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), ScreenshotError> {
    let state = (*state).clone();
    let label = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        state.scroll_capture(&label, monitor_index, x, y, width, height)
    })
    .await
    .map_err(|error| {
        eprintln!("screenshot scroll: {error}");
        ScreenshotError::new("capture_failed")
    })?
}

#[tauri::command(async)]
pub async fn screenshot_scroll_copy(
    state: State<'_, ScreenshotState>,
) -> Result<CopyResult, ScreenshotError> {
    let state = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || state.copy_frame())
        .await
        .map_err(|error| {
            eprintln!("screenshot scroll copy: {error}");
            ScreenshotError::new("storage")
        })?
}

#[tauri::command(async)]
pub async fn screenshot_scroll_save(
    state: State<'_, ScreenshotState>,
) -> Result<SaveResult, ScreenshotError> {
    let state = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || state.save_frame())
        .await
        .map_err(|error| {
            eprintln!("screenshot scroll save: {error}");
            ScreenshotError::new("storage")
        })?
}

#[tauri::command]
pub fn screenshot_copy(
    state: State<'_, ScreenshotState>,
    png_base64: String,
) -> Result<CopyResult, ScreenshotError> {
    state.copy_png(&png_base64)
}

#[tauri::command]
pub fn screenshot_save(
    state: State<'_, ScreenshotState>,
    png_base64: String,
) -> Result<SaveResult, ScreenshotError> {
    state.save_png(&png_base64)
}

#[tauri::command]
pub fn screenshot_adopt_pin(
    window: WebviewWindow,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale_factor: f64,
    crop_x: f64,
    crop_y: f64,
    page_width: f64,
    page_height: f64,
) -> Result<bool, ScreenshotError> {
    let app = window.app_handle().clone();
    super::windows::adopt_overlay(
        &app,
        window.label(),
        x.round() as i32,
        y.round() as i32,
        width.round().max(1.0) as u32,
        height.round().max(1.0) as u32,
        scale_factor,
        crop_x,
        crop_y,
        page_width,
        page_height,
    )
}

#[tauri::command]
pub fn screenshot_release_pin(window: WebviewWindow) -> Result<(), ScreenshotError> {
    let app = window.app_handle().clone();
    super::windows::release_overlay(&app, window.label())
}

#[tauri::command]
pub fn screenshot_pin(
    state: State<'_, ScreenshotState>,
    png_base64: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale_factor: f64,
) -> Result<u8, ScreenshotError> {
    state.pin(&png_base64, x, y, width, height, scale_factor)
}

#[tauri::command]
pub fn screenshot_pin_ready(state: State<'_, ScreenshotState>, slot: u8) {
    state.pin_ready(slot);
}

#[tauri::command]
pub fn screenshot_pin_image(
    state: State<'_, ScreenshotState>,
    slot: u8,
) -> Result<tauri::ipc::Response, ScreenshotError> {
    Ok(tauri::ipc::Response::new(state.pin_png(slot)?))
}

#[tauri::command]
pub fn screenshot_pin_copy(
    state: State<'_, ScreenshotState>,
    slot: u8,
) -> Result<CopyResult, ScreenshotError> {
    state.pin_copy(slot)
}

#[tauri::command]
pub fn screenshot_pin_close(
    state: State<'_, ScreenshotState>,
    slot: u8,
) -> Result<(), ScreenshotError> {
    state.pin_close(slot)
}

#[tauri::command]
pub fn screenshot_open_capture(app: AppHandle) {
    capture_from_app(&app);
}

pub fn screenshot_open_record(app: AppHandle) {
    super::session::record_from_hotkey(&app);
}

pub fn screenshot_menu_copy(app: &AppHandle, id: i64) {
    if let Some(state) = app.try_state::<super::session::ScreenshotState>() {
        let _ = state.library_copy(id);
    }
}

pub fn screenshot_menu_shots(app: &AppHandle) -> Vec<(i64, String)> {
    let Some(state) = app.try_state::<super::session::ScreenshotState>() else {
        return Vec::new();
    };
    state
        .library_list(None)
        .into_iter()
        .take(6)
        .map(|item| {
            let label = item
                .text
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .unwrap_or(item.name.as_str());
            (item.id, label.to_string())
        })
        .collect()
}

#[tauri::command]
pub fn screenshot_record_start(
    state: State<'_, ScreenshotState>,
    monitor_index: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    format: String,
    cursor: String,
    speaker: bool,
    microphone: bool,
    speaker_device: String,
    microphone_device: String,
) -> Result<(), ScreenshotError> {
    state.start_record(
        monitor_index,
        x,
        y,
        width,
        height,
        &format,
        &cursor,
        speaker,
        microphone,
        &speaker_device,
        &microphone_device,
    )
}

#[derive(serde::Serialize)]
pub struct AudioDevices {
    pub speakers: Vec<String>,
    pub microphones: Vec<String>,
}

#[tauri::command(async)]
pub async fn screenshot_audio_devices() -> AudioDevices {
    let (speakers, microphones) = tauri::async_runtime::spawn_blocking(super::audio::list_devices)
        .await
        .unwrap_or_default();
    AudioDevices {
        speakers,
        microphones,
    }
}

#[tauri::command]
pub fn screenshot_camera_open(
    state: State<'_, ScreenshotState>,
    monitor_index: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    device_id: String,
) -> Result<bool, ScreenshotError> {
    state.open_camera_preview(monitor_index, x, y, width, height, &device_id)
}

#[tauri::command]
pub fn screenshot_camera_close(app: AppHandle) {
    super::windows::close_camera(&app);
}

#[tauri::command]
pub fn screenshot_record_stop() {
    super::record::stop();
}

#[tauri::command]
pub fn screenshot_record_preview() -> Result<tauri::ipc::Response, ScreenshotError> {
    Ok(tauri::ipc::Response::new(super::record::preview_bytes()?))
}

#[tauri::command]
pub fn screenshot_record_download(app: AppHandle) -> Result<SaveResult, ScreenshotError> {
    let path = super::record::download_preview()?;
    if let Some(state) = app.try_state::<ScreenshotState>() {
        state.remember_file(std::path::Path::new(&path), None);
    }
    Ok(SaveResult { path })
}

#[tauri::command]
pub fn screenshot_record_copy(app: AppHandle) -> Result<(), ScreenshotError> {
    super::record::copy_preview(&app)
}

#[tauri::command]
pub fn screenshot_record_close(app: AppHandle) {
    super::record::close_preview(&app);
}

fn capture_from_app(app: &AppHandle) {
    super::session::capture_from_hotkey(app);
}

pub fn screenshot_is_enabled(app: &AppHandle) -> bool {
    app.try_state::<ScreenshotState>()
        .is_none_or(|state| state.settings().enabled)
}
