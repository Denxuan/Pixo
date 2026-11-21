use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::error::SshError;
use super::grant::{self, UploadPick};
use super::model::{Host, HostDraft, SshStatus};
use super::session::SshState;

#[tauri::command]
pub fn ssh_status(state: State<'_, SshState>) -> Result<SshStatus, SshError> {
    state.status()
}

#[tauri::command]
pub fn ssh_set_enabled(state: State<'_, SshState>, enabled: bool) -> Result<SshStatus, SshError> {
    state.set_enabled(enabled)
}

#[tauri::command]
pub fn ssh_hosts_list(state: State<'_, SshState>) -> Result<Vec<Host>, SshError> {
    state.list()
}

#[tauri::command]
pub fn ssh_host_save(state: State<'_, SshState>, host: HostDraft) -> Result<Host, SshError> {
    state.save(host)
}

#[tauri::command]
pub fn ssh_host_delete(state: State<'_, SshState>, id: i64) -> Result<(), SshError> {
    state.delete(id)
}

#[tauri::command]
pub fn ssh_host_forget_key(state: State<'_, SshState>, id: i64) -> Result<(), SshError> {
    state.forget_key(id)
}

#[tauri::command]
pub async fn ssh_pick_key(app: AppHandle, title: String) -> Result<Option<String>, SshError> {
    if title.chars().count() > 80 {
        return Err(SshError::new("invalid_input"));
    }
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title(title).blocking_pick_file()
    })
    .await
    .map_err(|_| SshError::new("storage"))?;
    match picked {
        Some(path) => {
            let path = path
                .into_path()
                .map_err(|_| SshError::new("invalid_input"))?;
            Ok(Some(path.to_string_lossy().into_owned()))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn ssh_connect(
    state: State<'_, SshState>,
    host_id: i64,
    cols: u32,
    rows: u32,
    output: Channel<InvokeResponseBody>,
    events: Channel<Value>,
) -> Result<u64, SshError> {
    state.connect(host_id, cols, rows, output, events)
}

#[tauri::command]
pub fn ssh_hostkey_decide(
    state: State<'_, SshState>,
    session_id: u64,
    accept: bool,
) -> Result<(), SshError> {
    state.decide_host_key(session_id, accept)
}

#[tauri::command]
pub fn ssh_answer(
    state: State<'_, SshState>,
    session_id: u64,
    prompt_id: u64,
    secret: Option<String>,
    remember: bool,
    cancel: bool,
) -> Result<(), SshError> {
    state.answer(session_id, prompt_id, secret, remember, cancel)
}

#[tauri::command]
pub fn ssh_write(
    state: State<'_, SshState>,
    session_id: u64,
    data: Vec<u8>,
) -> Result<(), SshError> {
    state.write(session_id, data)
}

#[tauri::command]
pub fn ssh_resize(
    state: State<'_, SshState>,
    session_id: u64,
    cols: u32,
    rows: u32,
) -> Result<(), SshError> {
    state.resize(session_id, cols, rows)
}

#[tauri::command]
pub fn ssh_disconnect(state: State<'_, SshState>, session_id: u64) -> Result<(), SshError> {
    state.disconnect(session_id)
}

#[tauri::command]
pub async fn ssh_pick_upload(
    app: AppHandle,
    kind: String,
    title: String,
) -> Result<Option<UploadPick>, SshError> {
    if title.chars().count() > 80 || (kind != "files" && kind != "folder") {
        return Err(SshError::new("invalid_input"));
    }
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file().set_title(title);
        if kind == "folder" {
            dialog.blocking_pick_folder().map(|path| vec![path])
        } else {
            dialog.blocking_pick_files()
        }
    })
    .await
    .map_err(|_| SshError::new("storage"))?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let mut paths = Vec::new();
    for path in picked {
        paths.push(
            path.into_path()
                .map_err(|_| SshError::new("invalid_input"))?,
        );
    }
    grant::issue(&paths).map(Some)
}

#[tauri::command]
pub fn ssh_release_upload(token: String) -> Result<(), SshError> {
    if token.len() > 64 {
        return Err(SshError::new("invalid_input"));
    }
    grant::release(&token);
    Ok(())
}

#[tauri::command]
pub fn ssh_transfer_start(
    state: State<'_, SshState>,
    session_id: u64,
    token: String,
    progress: Channel<Value>,
) -> Result<u64, SshError> {
    if token.len() > 64 {
        return Err(SshError::new("invalid_input"));
    }
    state.upload(session_id, token, progress)
}

#[tauri::command]
pub fn ssh_transfer_decide(
    state: State<'_, SshState>,
    task_id: u64,
    policy: String,
    apply_all: bool,
) -> Result<(), SshError> {
    if policy.len() > 16 {
        return Err(SshError::new("invalid_input"));
    }
    state.decide_transfer(task_id, &policy, apply_all)
}

#[tauri::command]
pub fn ssh_transfer_cancel(state: State<'_, SshState>, task_id: u64) -> Result<(), SshError> {
    state.cancel_transfer(task_id)
}
