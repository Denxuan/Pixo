use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::error::SshError;
use super::grant::{self, UploadPick};
use super::model::{ForwardView, Host, HostDraft, Snippet, SnippetDraft, SshStatus};
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
pub fn ssh_set_keepalive(state: State<'_, SshState>, seconds: u32) -> Result<SshStatus, SshError> {
    state.set_keepalive(seconds)
}

#[tauri::command]
pub fn ssh_known_hosts(state: State<'_, SshState>) -> Vec<super::known_hosts::TrustedHost> {
    state.trusted_hosts()
}

#[tauri::command]
pub fn ssh_known_host_forget(
    state: State<'_, SshState>,
    host: String,
    port: u16,
) -> Result<(), SshError> {
    state.forget_trusted(&host, port)
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
pub fn ssh_host_delete(
    state: State<'_, SshState>,
    id: i64,
    detach: Option<bool>,
) -> Result<(), SshError> {
    state.delete(id, detach.unwrap_or(false))
}

#[tauri::command]
pub fn ssh_config_preview(
    state: State<'_, SshState>,
) -> Result<Vec<super::config_import::ConfigPreview>, SshError> {
    state.config_preview()
}

#[tauri::command]
pub fn ssh_config_import(
    state: State<'_, SshState>,
    indexes: Vec<u32>,
) -> Result<Vec<Host>, SshError> {
    if indexes.len() > 500 {
        return Err(SshError::new("invalid_input"));
    }
    state.config_import(&indexes)
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
pub fn ssh_interrupt(state: State<'_, SshState>, session_id: u64) -> Result<(), SshError> {
    state.interrupt(session_id)
}

#[tauri::command]
pub fn ssh_ack(state: State<'_, SshState>, session_id: u64, bytes: u64) -> Result<(), SshError> {
    state.ack(session_id, bytes)
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
    direction: Option<String>,
    remote_paths: Option<Vec<String>>,
) -> Result<u64, SshError> {
    if token.len() > 64 {
        return Err(SshError::new("invalid_input"));
    }
    if direction.as_deref() == Some("download") {
        return state.download(
            session_id,
            token,
            remote_paths.unwrap_or_default(),
            progress,
        );
    }
    state.upload(session_id, token, progress)
}

#[tauri::command]
pub async fn ssh_list_dir(
    state: State<'_, SshState>,
    session_id: u64,
    path: String,
) -> Result<super::transfer::RemoteListing, SshError> {
    if path.len() > 4096 {
        return Err(SshError::new("invalid_input"));
    }
    state.list_dir(session_id, path).await
}

#[tauri::command]
pub async fn ssh_make_dir(
    state: State<'_, SshState>,
    session_id: u64,
    path: String,
) -> Result<(), SshError> {
    if path.len() > 4096 {
        return Err(SshError::new("invalid_input"));
    }
    state.make_dir(session_id, path).await
}

#[tauri::command]
pub async fn ssh_rename(
    state: State<'_, SshState>,
    session_id: u64,
    from: String,
    to: String,
) -> Result<(), SshError> {
    if from.len() > 4096 || to.len() > 4096 {
        return Err(SshError::new("invalid_input"));
    }
    state.rename_path(session_id, from, to).await
}

#[tauri::command]
pub async fn ssh_remove(
    state: State<'_, SshState>,
    session_id: u64,
    paths: Vec<String>,
) -> Result<(), SshError> {
    if paths.len() > 100 || paths.iter().any(|path| path.len() > 4096) {
        return Err(SshError::new("invalid_input"));
    }
    state.remove_paths(session_id, paths).await
}

#[tauri::command]
pub async fn ssh_pick_download_dir(
    app: AppHandle,
    title: String,
) -> Result<Option<UploadPick>, SshError> {
    if title.chars().count() > 80 {
        return Err(SshError::new("invalid_input"));
    }
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title(title).blocking_pick_folder()
    })
    .await
    .map_err(|_| SshError::new("storage"))?;
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|_| SshError::new("invalid_input"))?;
    grant::issue(std::slice::from_ref(&path)).map(Some)
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

#[tauri::command]
pub fn ssh_transfer_retry(
    state: State<'_, SshState>,
    session_id: u64,
    task_id: u64,
    progress: Channel<Value>,
) -> Result<u64, SshError> {
    state.retry_transfer(session_id, task_id, progress)
}

#[tauri::command]
pub fn ssh_transfer_forget(state: State<'_, SshState>, task_id: u64) -> Result<(), SshError> {
    state.forget_transfer(task_id);
    Ok(())
}

#[tauri::command]
pub async fn ssh_forward_open(
    state: State<'_, SshState>,
    session_id: u64,
    kind: String,
    listen_port: u32,
    target_host: String,
    target_port: u32,
) -> Result<ForwardView, SshError> {
    state
        .open_forward(session_id, &kind, listen_port, &target_host, target_port)
        .await
}

#[tauri::command]
pub fn ssh_forward_list(
    state: State<'_, SshState>,
    session_id: u64,
) -> Result<Vec<ForwardView>, SshError> {
    state.list_forwards(session_id)
}

#[tauri::command]
pub fn ssh_forward_close(
    state: State<'_, SshState>,
    session_id: u64,
    id: u64,
) -> Result<(), SshError> {
    state.close_forward(session_id, id)
}

#[tauri::command]
pub fn ssh_snippets_list(state: State<'_, SshState>) -> Result<Vec<Snippet>, SshError> {
    state.snippets()
}

#[tauri::command]
pub fn ssh_snippet_save(
    state: State<'_, SshState>,
    snippet: SnippetDraft,
) -> Result<Snippet, SshError> {
    state.save_snippet(&snippet)
}

#[tauri::command]
pub fn ssh_snippet_delete(state: State<'_, SshState>, id: i64) -> Result<(), SshError> {
    state.delete_snippet(id)
}
