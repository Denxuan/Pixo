use tauri::Manager;

pub(crate) mod commands;
mod compose;
mod context;
mod error;
mod frames;
mod model;
mod service;
mod sockets;
mod store;

pub use service::initialize;

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
