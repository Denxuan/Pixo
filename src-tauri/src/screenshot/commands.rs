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

#[tauri::command]
pub fn screenshot_start(
    state: State<'_, ScreenshotState>,
    mode: Option<String>,
    hide_self: Option<bool>,
) -> Result<SessionView, ScreenshotError> {
    let settings = state.settings();
    state.start(
        mode.as_deref().unwrap_or("capture"),
        hide_self.unwrap_or(settings.hide_self),
    )
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
) -> Result<(), ScreenshotError> {
    if !window.label().starts_with("screenshot-overlay-") || !state.has_session() {
        return Ok(());
    }
    super::windows::reveal(&window);
    Ok(())
}

#[tauri::command]
pub fn screenshot_frame(
    state: State<'_, ScreenshotState>,
    index: u32,
) -> Result<tauri::ipc::Response, ScreenshotError> {
    Ok(tauri::ipc::Response::new(state.frame(index)?))
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
pub fn screenshot_copy_text(text: String) -> Result<(), ScreenshotError> {
    super::ocr::copy_text(&text)
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

#[tauri::command]
pub fn screenshot_record_start(
    state: State<'_, ScreenshotState>,
    monitor_index: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    format: String,
) -> Result<(), ScreenshotError> {
    state.start_record(monitor_index, x, y, width, height, &format)
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
pub fn screenshot_record_download() -> Result<SaveResult, ScreenshotError> {
    Ok(SaveResult {
        path: super::record::download_preview()?,
    })
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
