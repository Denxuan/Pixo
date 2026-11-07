use tauri::State;

use super::error::ClipboardError;
use super::model::{
    ClipKind, CopyOutcome, Detail, InstalledApp, ListPage, Settings, SettingsUpdate, SourceMark,
    Status, Summary,
};
use super::service::ClipboardState;

#[tauri::command]
pub fn clipboard_list(
    state: State<'_, ClipboardState>,
    query: Option<String>,
    kind: Option<String>,
    pinned_only: Option<bool>,
    cursor: Option<String>,
    limit: Option<u32>,
) -> Result<ListPage, ClipboardError> {
    let kind = match kind
        .as_deref()
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
    {
        None => None,
        Some(kind) => {
            Some(ClipKind::parse(kind).ok_or_else(|| ClipboardError::new("invalid_input"))?)
        }
    };
    let limit = limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ClipboardError::new("invalid_input"));
    }
    state.list(
        query.as_deref(),
        kind,
        pinned_only.unwrap_or(false),
        cursor.as_deref(),
        limit,
    )
}

#[tauri::command]
pub fn clipboard_get(state: State<'_, ClipboardState>, id: i64) -> Result<Detail, ClipboardError> {
    state.get(id)
}

#[tauri::command]
pub fn clipboard_source_icon(source: String) -> Result<Option<SourceMark>, ClipboardError> {
    Ok(super::monitor::source_mark(&source))
}

#[tauri::command]
pub fn clipboard_installed_apps() -> Result<Vec<InstalledApp>, ClipboardError> {
    Ok(super::monitor::installed_apps())
}

#[tauri::command]
pub fn clipboard_image(
    state: State<'_, ClipboardState>,
    id: i64,
    thumb: Option<bool>,
) -> Result<tauri::ipc::Response, ClipboardError> {
    let bytes = state.image(id, thumb.unwrap_or(false))?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub fn clipboard_copy(
    state: State<'_, ClipboardState>,
    id: i64,
) -> Result<CopyOutcome, ClipboardError> {
    state.copy(id)
}

#[tauri::command]
pub fn clipboard_set_pinned(
    state: State<'_, ClipboardState>,
    id: i64,
    pinned: bool,
) -> Result<(), ClipboardError> {
    state.set_pinned(id, pinned)
}

#[tauri::command]
pub fn clipboard_delete(
    state: State<'_, ClipboardState>,
    ids: Vec<i64>,
) -> Result<(), ClipboardError> {
    state.delete(ids)
}

#[tauri::command]
pub fn clipboard_clear(
    state: State<'_, ClipboardState>,
    keep_pinned: bool,
) -> Result<(), ClipboardError> {
    state.clear(keep_pinned)
}

#[tauri::command]
pub fn clipboard_read_now(state: State<'_, ClipboardState>) -> Result<Summary, ClipboardError> {
    state.read_now()
}

#[tauri::command]
pub fn clipboard_status(state: State<'_, ClipboardState>) -> Result<Status, ClipboardError> {
    Ok(state.status())
}

#[tauri::command]
pub fn clipboard_set_paused(
    state: State<'_, ClipboardState>,
    paused: bool,
) -> Result<Status, ClipboardError> {
    state.set_paused(paused)
}

#[tauri::command]
pub fn clipboard_get_settings(
    state: State<'_, ClipboardState>,
) -> Result<Settings, ClipboardError> {
    Ok(state.settings())
}

#[tauri::command]
pub fn clipboard_update_settings(
    state: State<'_, ClipboardState>,
    settings: SettingsUpdate,
) -> Result<Settings, ClipboardError> {
    state.update_settings(settings)
}
