use tauri::Manager;

pub(crate) mod commands;
mod compose;
mod context;
mod error;
mod frames;
mod hostdir;
mod mirrors;
mod model;
mod registry;
mod service;
mod sockets;
mod store;
mod updates;

pub use service::initialize;

pub(crate) fn live_links(app: &tauri::AppHandle) -> usize {
    app.try_state::<service::DockerState>()
        .map(|state| state.link_count())
        .unwrap_or(0)
}

pub(crate) fn endpoint_names_for_host(app: &tauri::AppHandle, host_id: i64) -> Vec<String> {
    app.try_state::<service::DockerState>()
        .map(|state| state.endpoint_names_for_host(host_id))
        .unwrap_or_default()
}

pub(crate) fn endpoints_json(app: &tauri::AppHandle) -> Result<String, String> {
    let state = app.try_state::<service::DockerState>().ok_or("storage")?;
    let endpoints = state.stored().map_err(|error| error.code.to_string())?;
    serde_json::to_string_pretty(&endpoints).map_err(|_| "storage".to_string())
}

pub(crate) fn menu_endpoints(app: &tauri::AppHandle) -> (bool, Vec<(i64, String)>) {
    let Some(state) = app.try_state::<service::DockerState>() else {
        return (false, Vec::new());
    };
    let enabled = state.status().map(|status| status.enabled).unwrap_or(false);
    if !enabled {
        return (false, Vec::new());
    }
    let items = state
        .list()
        .unwrap_or_default()
        .into_iter()
        .take(8)
        .map(|endpoint| (endpoint.id, endpoint.name))
        .collect();
    (true, items)
}
