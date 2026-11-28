use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::compose::{ComposePreview, ComposeProject, ComposeProjectRequest};
use super::context::ContextPreview;
use super::error::DockerError;
use super::model::{
    ContainerRow, DockerStatus, Endpoint, EndpointDraft, EngineSummary, HostDirListing, ImageRow,
    NetworkRow,
};
use super::service::DockerState;
use super::sockets::SocketCandidate;

#[tauri::command]
pub fn docker_status(state: State<'_, DockerState>) -> Result<DockerStatus, DockerError> {
    state.status()
}

#[tauri::command]
pub fn docker_set_enabled(
    state: State<'_, DockerState>,
    enabled: bool,
) -> Result<DockerStatus, DockerError> {
    state.set_enabled(enabled)
}

#[tauri::command]
pub fn docker_set_image_updates(
    state: State<'_, DockerState>,
    enabled: bool,
) -> Result<DockerStatus, DockerError> {
    state.set_image_updates(enabled)
}

#[tauri::command]
pub fn docker_endpoints_list(state: State<'_, DockerState>) -> Result<Vec<Endpoint>, DockerError> {
    state.list()
}

#[tauri::command]
pub fn docker_endpoint_save(
    state: State<'_, DockerState>,
    endpoint: EndpointDraft,
) -> Result<Endpoint, DockerError> {
    state.save(endpoint)
}

#[tauri::command]
pub fn docker_endpoint_delete(state: State<'_, DockerState>, id: i64) -> Result<(), DockerError> {
    state.delete(id)
}

#[tauri::command]
pub fn docker_local_sockets(
    state: State<'_, DockerState>,
) -> Result<Vec<SocketCandidate>, DockerError> {
    state.sockets()
}

#[tauri::command]
pub fn docker_contexts_preview(
    state: State<'_, DockerState>,
) -> Result<Vec<ContextPreview>, DockerError> {
    state.context_preview()
}

#[tauri::command]
pub fn docker_contexts_import(
    state: State<'_, DockerState>,
    indexes: Vec<u32>,
) -> Result<Vec<Endpoint>, DockerError> {
    state.context_import(&indexes)
}

#[tauri::command]
pub async fn docker_pick_file(
    app: AppHandle,
    title: String,
) -> Result<Option<String>, DockerError> {
    if title.chars().count() > 80 {
        return Err(DockerError::new("invalid_input"));
    }
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title(title).blocking_pick_file()
    })
    .await
    .map_err(|_| DockerError::new("storage"))?;
    match picked {
        Some(path) => {
            let path = path
                .into_path()
                .map_err(|_| DockerError::new("invalid_input"))?;
            Ok(Some(path.to_string_lossy().into_owned()))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn docker_pick_folder(
    app: AppHandle,
    title: String,
) -> Result<Option<String>, DockerError> {
    if title.chars().count() > 80 {
        return Err(DockerError::new("invalid_input"));
    }
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title(title).blocking_pick_folder()
    })
    .await
    .map_err(|_| DockerError::new("storage"))?;
    match picked {
        Some(path) => {
            let path = path
                .into_path()
                .map_err(|_| DockerError::new("invalid_input"))?;
            Ok(Some(path.to_string_lossy().into_owned()))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn docker_connect(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    events: Channel<Value>,
) -> Result<u64, DockerError> {
    state.connect(endpoint_id, events)
}

#[tauri::command]
pub fn docker_disconnect(
    state: State<'_, DockerState>,
    endpoint_id: i64,
) -> Result<(), DockerError> {
    state.disconnect(endpoint_id)
}

#[tauri::command]
pub fn docker_hostkey_decide(
    state: State<'_, DockerState>,
    link_id: u64,
    accept: bool,
) -> Result<(), DockerError> {
    state.decide_hostkey(link_id, accept)
}

#[tauri::command]
pub fn docker_answer(
    state: State<'_, DockerState>,
    link_id: u64,
    prompt_id: u64,
    secret: Option<String>,
    remember: bool,
    cancel: bool,
) -> Result<(), DockerError> {
    state.answer(link_id, prompt_id, secret, remember, cancel)
}

#[tauri::command]
pub async fn docker_summary(
    state: State<'_, DockerState>,
    endpoint_id: i64,
) -> Result<EngineSummary, DockerError> {
    state.summary(endpoint_id).await
}

#[tauri::command]
pub async fn docker_mirrors(
    state: State<'_, DockerState>,
    endpoint_id: i64,
) -> Result<Vec<String>, DockerError> {
    state.mirrors(endpoint_id).await
}

#[tauri::command]
pub async fn docker_set_mirrors(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    mirrors: Vec<String>,
) -> Result<(), DockerError> {
    state.set_mirrors(endpoint_id, mirrors).await
}

#[tauri::command]
pub async fn docker_renew(
    state: State<'_, DockerState>,
    endpoint_id: i64,
) -> Result<(), DockerError> {
    state.renew(endpoint_id).await
}

#[tauri::command]
pub async fn docker_containers(
    state: State<'_, DockerState>,
    endpoint_id: i64,
) -> Result<Vec<ContainerRow>, DockerError> {
    state.containers(endpoint_id).await
}

#[tauri::command]
pub async fn docker_container_action(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    id: String,
    action: String,
    force: Option<bool>,
) -> Result<(), DockerError> {
    state
        .container_action(endpoint_id, &id, &action, force.unwrap_or(false))
        .await
}

#[tauri::command]
pub async fn docker_container_inspect(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    id: String,
) -> Result<Value, DockerError> {
    state.container_inspect(endpoint_id, &id).await
}

#[tauri::command]
pub fn docker_container_logs(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    id: String,
    events: Channel<Value>,
) -> Result<u64, DockerError> {
    state.container_logs(endpoint_id, &id, events)
}

#[tauri::command]
pub fn docker_task_stop(state: State<'_, DockerState>, task_id: u64) -> Result<(), DockerError> {
    state.stop_task(task_id)
}

#[tauri::command]
pub async fn docker_images(
    state: State<'_, DockerState>,
    endpoint_id: i64,
) -> Result<Vec<ImageRow>, DockerError> {
    state.images(endpoint_id).await
}

#[tauri::command]
pub async fn docker_image_inspect(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    name: String,
) -> Result<Value, DockerError> {
    state.image_inspect(endpoint_id, &name).await
}

#[tauri::command]
pub async fn docker_image_remove(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    name: String,
) -> Result<(), DockerError> {
    state.image_remove(endpoint_id, &name).await
}

#[tauri::command]
pub async fn docker_exec_open(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    container_id: String,
    cols: u32,
    rows: u32,
    command: String,
    output: Channel<InvokeResponseBody>,
    events: Channel<Value>,
) -> Result<u64, DockerError> {
    state
        .exec_open(
            endpoint_id,
            &container_id,
            cols,
            rows,
            &command,
            output,
            events,
        )
        .await
}

#[tauri::command]
pub fn docker_exec_write(
    state: State<'_, DockerState>,
    id: u64,
    data: Vec<u8>,
) -> Result<(), DockerError> {
    state.exec_write(id, data)
}

#[tauri::command]
pub fn docker_exec_resize(
    state: State<'_, DockerState>,
    id: u64,
    cols: u32,
    rows: u32,
) -> Result<(), DockerError> {
    state.exec_resize(id, cols, rows)
}

#[tauri::command]
pub fn docker_exec_close(state: State<'_, DockerState>, id: u64) -> Result<(), DockerError> {
    state.exec_close(id)
}

#[tauri::command]
pub async fn docker_image_run(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    request: super::model::ContainerRunRequest,
) -> Result<String, DockerError> {
    state.image_run(endpoint_id, request).await
}

#[tauri::command]
pub async fn docker_container_stats(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    id: String,
) -> Result<super::model::ContainerUsage, DockerError> {
    state.container_stats(endpoint_id, &id).await
}

#[tauri::command]
pub async fn docker_image_updates(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    events: Channel<Value>,
) -> Result<(), DockerError> {
    state.image_updates(endpoint_id, events).await
}

#[tauri::command]
pub async fn docker_image_users(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    reference: String,
) -> Result<Vec<String>, DockerError> {
    state.image_users(endpoint_id, &reference).await
}

#[tauri::command]
pub fn docker_image_pull(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    reference: String,
    username: Option<String>,
    password: Option<String>,
    remember: bool,
    events: Channel<Value>,
) -> Result<u64, DockerError> {
    state.image_pull(
        endpoint_id,
        &reference,
        username,
        password,
        remember,
        events,
    )
}

#[tauri::command]
pub async fn docker_networks(
    state: State<'_, DockerState>,
    endpoint_id: i64,
) -> Result<Vec<NetworkRow>, DockerError> {
    state.networks(endpoint_id).await
}

#[tauri::command]
pub async fn docker_network_inspect(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    name: String,
) -> Result<Value, DockerError> {
    state.network_inspect(endpoint_id, &name).await
}

#[tauri::command]
pub async fn docker_network_remove(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    name: String,
) -> Result<(), DockerError> {
    state.network_remove(endpoint_id, &name).await
}

#[tauri::command]
pub async fn docker_list_dirs(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    path: String,
) -> Result<HostDirListing, DockerError> {
    state.list_host_dirs(endpoint_id, &path).await
}

#[tauri::command]
pub async fn docker_compose_projects(
    state: State<'_, DockerState>,
    endpoint_id: i64,
) -> Result<Vec<ComposeProject>, DockerError> {
    state.compose_projects(endpoint_id).await
}

#[tauri::command]
pub fn docker_compose_preview(
    state: State<'_, DockerState>,
    path: String,
) -> Result<ComposePreview, DockerError> {
    state.compose_preview(&path)
}

#[tauri::command]
pub async fn docker_compose_create(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    draft: ComposeProjectRequest,
) -> Result<(), DockerError> {
    state.compose_create(endpoint_id, draft).await
}

#[tauri::command]
pub fn docker_compose_import(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    path: String,
) -> Result<(), DockerError> {
    state.compose_import_file(endpoint_id, &path)
}

#[tauri::command]
pub async fn docker_compose_import_engine(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    name: String,
) -> Result<(), DockerError> {
    state.compose_import_engine(endpoint_id, &name).await
}

#[tauri::command]
pub fn docker_compose_forget(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    id: i64,
) -> Result<(), DockerError> {
    state.compose_forget(endpoint_id, id)
}

#[tauri::command]
pub async fn docker_compose_action(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    name: String,
    action: String,
) -> Result<(), DockerError> {
    state.compose_action(endpoint_id, &name, &action).await
}

#[tauri::command]
pub async fn docker_compose_remove(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    name: String,
) -> Result<(), DockerError> {
    state.compose_remove(endpoint_id, &name).await
}

#[tauri::command]
pub async fn docker_compose_file(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    name: String,
) -> Result<String, DockerError> {
    state.compose_file(endpoint_id, &name).await
}

#[tauri::command]
pub async fn docker_compose_logs(
    state: State<'_, DockerState>,
    endpoint_id: i64,
    name: String,
    events: Channel<Value>,
) -> Result<u64, DockerError> {
    state.compose_logs(endpoint_id, &name, events).await
}
