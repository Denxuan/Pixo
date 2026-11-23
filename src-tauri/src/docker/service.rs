use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bollard::Docker;
use bollard::query_parameters::{
    CreateImageOptionsBuilder, ListContainersOptionsBuilder, ListImagesOptionsBuilder, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, RemoveImageOptionsBuilder, RestartContainerOptionsBuilder, StopContainerOptionsBuilder,
};
use futures_util::StreamExt;
use russh::Disconnect;
use russh::client;
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::watch;

use crate::ssh::dial::{self, ClientHandler};
use crate::ssh::SshState;

use super::compose::{self, ComposePreview, ComposeProject, LabeledContainer};
use super::context::{self, ContextPreview};
use super::error::{DockerError, log_docker};
use super::model::{
    ContainerRow, DockerStatus, Endpoint, EndpointDraft, EndpointKind, EngineSummary, ImageRow, NetworkRow, NormalizedEndpoint,
    VolumeRow, normalize, valid_image_ref, valid_resource_name,
};
use super::sockets::{self, SocketCandidate};
use super::store::Store;

const IDLE: Duration = Duration::from_secs(60);
const API_TIMEOUT: u64 = 30;

struct LinkControl {
    endpoint_id: i64,
    bridge: Arc<dial::Bridge>,
}

struct ClientSlot {
    docker: Docker,
    // Held so the SSH proxy stays up for the lifetime of the client.
    #[allow(dead_code)]
    proxy: Option<Proxy>,
    socket: Option<String>,
    last_used: Instant,
}

struct RunningTask {
    endpoint_id: i64,
    cancel: watch::Sender<bool>,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl Drop for RunningTask {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        self.task.abort();
    }
}

struct Proxy {
    cancel: watch::Sender<bool>,
    task: tauri::async_runtime::JoinHandle<()>,
    session: Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>,
    jumps: Vec<client::Handle<ClientHandler>>,
}

impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        self.task.abort();
        let session = Arc::clone(&self.session);
        let jumps = std::mem::take(&mut self.jumps);
        tauri::async_runtime::spawn(async move {
            if let Some(handle) = session.lock().await.take() {
                let _ = handle.disconnect(Disconnect::ByApplication, "", "").await;
            }
            for jump in jumps {
                let _ = jump.disconnect(Disconnect::ByApplication, "", "").await;
            }
        });
    }
}

struct Inner {
    app: AppHandle,
    store: Mutex<Store>,
    links: Mutex<HashMap<u64, LinkControl>>,
    clients: Mutex<HashMap<i64, ClientSlot>>,
    tasks: Mutex<HashMap<u64, RunningTask>>,
    next_id: AtomicU64,
}

pub struct DockerState {
    inner: Arc<Inner>,
}

pub fn initialize(app: AppHandle) -> Result<DockerState, DockerError> {
    let root = app.path().app_data_dir().map_err(|error| {
        eprintln!("docker path: {error}");
        DockerError::new("storage")
    })?;
    let dir = root.join("docker");
    std::fs::create_dir_all(&dir).map_err(storage_io)?;
    let database = dir.join("endpoints.db");
    let store = Store::open(&database)?;
    tighten(&database);
    Ok(DockerState {
        inner: Arc::new(Inner {
            app,
            store: Mutex::new(store),
            links: Mutex::new(HashMap::new()),
            clients: Mutex::new(HashMap::new()),
            tasks: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }),
    })
}

impl DockerState {
    pub fn status(&self) -> Result<DockerStatus, DockerError> {
        Ok(DockerStatus {
            enabled: lock(&self.inner.store).enabled()?,
        })
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<DockerStatus, DockerError> {
        lock(&self.inner.store).set_enabled(enabled)?;
        if !enabled {
            self.shutdown();
        }
        let _ = self.inner.app.emit("docker://enabled", enabled);
        self.touch_menu();
        self.status()
    }

    pub fn list(&self) -> Result<Vec<Endpoint>, DockerError> {
        self.require()?;
        lock(&self.inner.store).list()
    }

    pub fn save(&self, draft: EndpointDraft) -> Result<Endpoint, DockerError> {
        self.require()?;
        let endpoint = normalize(draft)?;
        if endpoint.kind == EndpointKind::Ssh {
            self.ensure_ssh_host(endpoint.ssh_host_id.unwrap_or(0))?;
        }
        let saved = lock(&self.inner.store).save(&endpoint, "manual", now_millis())?;
        if let Some(id) = endpoint.id {
            self.drop_client(id);
        }
        self.touch_menu();
        Ok(saved)
    }

    pub fn delete(&self, id: i64) -> Result<(), DockerError> {
        self.require()?;
        lock(&self.inner.store).delete(id)?;
        self.drop_client(id);
        self.touch_menu();
        Ok(())
    }

    pub fn sockets(&self) -> Result<Vec<SocketCandidate>, DockerError> {
        self.require()?;
        Ok(sockets::discover_sockets())
    }

    pub fn context_preview(&self) -> Result<Vec<ContextPreview>, DockerError> {
        self.require()?;
        context::load_previews(&self.ssh_host_keys())
    }

    pub fn context_import(&self, indexes: &[u32]) -> Result<Vec<Endpoint>, DockerError> {
        self.require()?;
        let chosen = context::selected_contexts(indexes, &self.ssh_host_keys())?;
        let existing = lock(&self.inner.store).list()?;
        let mut saved = Vec::new();
        for endpoint in chosen {
            if endpoint.kind == EndpointKind::Ssh {
                self.ensure_ssh_host(endpoint.ssh_host_id.unwrap_or(0))?;
            }
            if existing.iter().any(|item| signature_saved(item) == signature_new(&endpoint)) {
                continue;
            }
            saved.push(lock(&self.inner.store).save(&endpoint, "context", now_millis())?);
        }
        if saved.is_empty() {
            return Err(DockerError::new("invalid_input"));
        }
        self.touch_menu();
        Ok(saved)
    }

    pub fn connect(&self, endpoint_id: i64, events: Channel<Value>) -> Result<u64, DockerError> {
        self.require()?;
        let endpoint = lock(&self.inner.store).get(endpoint_id)?;
        self.drop_client(endpoint_id);
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let bridge = dial::Bridge::new();
        lock(&self.inner.links).insert(
            id,
            LinkControl {
                endpoint_id,
                bridge: Arc::clone(&bridge),
            },
        );
        let inner = Arc::clone(&self.inner);
        tauri::async_runtime::spawn(async move {
            let result = open_and_store(&inner, &endpoint, &bridge, &events).await;
            lock(&inner.links).remove(&id);
            if let Err(error) = result {
                log_docker("connect", error.code);
                emit(
                    &events,
                    json!({"type": "closed", "code": error.code, "params": error.params}),
                );
            }
        });
        Ok(id)
    }

    pub fn disconnect(&self, endpoint_id: i64) -> Result<(), DockerError> {
        self.require()?;
        self.drop_client(endpoint_id);
        Ok(())
    }

    pub fn decide_hostkey(&self, link_id: u64, accept: bool) -> Result<(), DockerError> {
        self.require()?;
        let links = lock(&self.inner.links);
        let link = links.get(&link_id).ok_or_else(|| DockerError::new("not_found"))?;
        dial::decide_hostkey(&link.bridge, accept)?;
        Ok(())
    }

    pub fn answer(
        &self,
        link_id: u64,
        prompt_id: u64,
        secret: Option<String>,
        remember: bool,
        cancel: bool,
    ) -> Result<(), DockerError> {
        self.require()?;
        let links = lock(&self.inner.links);
        let link = links.get(&link_id).ok_or_else(|| DockerError::new("not_found"))?;
        dial::answer_prompt(&link.bridge, prompt_id, secret, remember, cancel)?;
        Ok(())
    }

    pub async fn summary(&self, endpoint_id: i64) -> Result<EngineSummary, DockerError> {
        self.require()?;
        let (docker, socket) = self.client(endpoint_id)?;
        read_summary(&docker, socket).await
    }

    pub async fn containers(&self, endpoint_id: i64) -> Result<Vec<ContainerRow>, DockerError> {
        self.require()?;
        let (docker, _) = self.client(endpoint_id)?;
        let options = ListContainersOptionsBuilder::new().all(true).build();
        let rows = docker
            .list_containers(Some(options))
            .await
            .map_err(|error| map_bollard(error, false))?;
        Ok(rows
            .into_iter()
            .map(|item| ContainerRow {
                id: item.id.unwrap_or_default(),
                name: item
                    .names
                    .and_then(|names| names.into_iter().next())
                    .unwrap_or_default()
                    .trim_start_matches('/')
                    .to_string(),
                image: item.image.unwrap_or_default(),
                state: item.state.map(|state| state.to_string()).unwrap_or_default(),
                status: item.status.unwrap_or_default(),
                ports: item.ports.unwrap_or_default().into_iter().map(port_label).collect(),
                created: item.created.unwrap_or(0),
            })
            .collect())
    }

    pub async fn container_action(&self, endpoint_id: i64, id: &str, action: &str, force: bool) -> Result<(), DockerError> {
        self.require()?;
        let id = resource_name(id)?;
        let (docker, _) = self.client(endpoint_id)?;
        let result = match action {
            "start" => docker.start_container(id, None).await,
            "stop" => docker.stop_container(id, Some(StopContainerOptionsBuilder::new().t(10).build())).await,
            "restart" => docker.restart_container(id, Some(RestartContainerOptionsBuilder::new().t(10).build())).await,
            "remove" => {
                docker
                    .remove_container(id, Some(RemoveContainerOptionsBuilder::new().force(force).build()))
                    .await
            }
            _ => return Err(DockerError::new("invalid_input")),
        };
        result.map_err(|error| map_bollard(error, false))
    }

    pub async fn container_inspect(&self, endpoint_id: i64, id: &str) -> Result<Value, DockerError> {
        self.require()?;
        let id = resource_name(id)?;
        let (docker, _) = self.client(endpoint_id)?;
        let value = docker.inspect_container(id, None).await.map_err(|error| map_bollard(error, false))?;
        serde_json::to_value(value).map_err(|_| DockerError::new("api"))
    }

    pub fn container_logs(&self, endpoint_id: i64, id: &str, events: Channel<Value>) -> Result<u64, DockerError> {
        self.require()?;
        let id = resource_name(id)?.to_string();
        let (docker, _) = self.client(endpoint_id)?;
        self.spawn_task(endpoint_id, events, move |events, mut cancel| async move {
            let options = LogsOptionsBuilder::new().follow(true).stdout(true).stderr(true).tail("200").build();
            let mut stream = docker.logs(&id, Some(options));
            loop {
                tokio::select! {
                    _ = cancel.wait_for(|stopped| *stopped) => break,
                    item = stream.next() => {
                        match item {
                            Some(Ok(output)) => emit_log(&events, &output),
                            Some(Err(error)) => {
                                let error = map_bollard(error, false);
                                emit(&events, json!({"type": "closed", "code": error.code, "params": error.params}));
                                return;
                            }
                            None => break,
                        }
                    }
                }
            }
            emit(&events, json!({"type": "closed", "code": "ok", "params": {}}));
        })
    }

    pub fn stop_task(&self, task_id: u64) -> Result<(), DockerError> {
        self.require()?;
        lock(&self.inner.tasks)
            .remove(&task_id)
            .map(|_| ())
            .ok_or_else(|| DockerError::new("not_found"))
    }

    pub async fn images(&self, endpoint_id: i64) -> Result<Vec<ImageRow>, DockerError> {
        self.require()?;
        let (docker, _) = self.client(endpoint_id)?;
        let options = ListImagesOptionsBuilder::new().all(true).build();
        let rows = docker.list_images(Some(options)).await.map_err(|error| map_bollard(error, false))?;
        Ok(rows
            .into_iter()
            .map(|item| ImageRow {
                id: item.id,
                tags: item.repo_tags.into_iter().filter(|tag| tag != "<none>:<none>").collect(),
                size: item.size,
                created: item.created,
            })
            .collect())
    }

    pub async fn image_inspect(&self, endpoint_id: i64, name: &str) -> Result<Value, DockerError> {
        self.require()?;
        let name = image_name(name)?;
        let (docker, _) = self.client(endpoint_id)?;
        let value = docker.inspect_image(name).await.map_err(|error| map_bollard(error, false))?;
        serde_json::to_value(value).map_err(|_| DockerError::new("api"))
    }

    pub async fn image_remove(&self, endpoint_id: i64, name: &str) -> Result<(), DockerError> {
        self.require()?;
        let name = image_name(name)?;
        let (docker, _) = self.client(endpoint_id)?;
        docker
            .remove_image(name, Some(RemoveImageOptionsBuilder::new().build()), None)
            .await
            .map(|_| ())
            .map_err(|error| map_bollard(error, true))
    }

    pub fn image_pull(&self, endpoint_id: i64, reference: &str, events: Channel<Value>) -> Result<u64, DockerError> {
        self.require()?;
        let reference = image_name(reference)?.to_string();
        let (docker, _) = self.client(endpoint_id)?;
        self.spawn_task(endpoint_id, events, move |events, mut cancel| async move {
            let options = CreateImageOptionsBuilder::new().from_image(&reference).build();
            let mut stream = docker.create_image(Some(options), None, None);
            loop {
                tokio::select! {
                    _ = cancel.wait_for(|stopped| *stopped) => break,
                    item = stream.next() => {
                        match item {
                            Some(Ok(info)) => {
                                let _ = events.send(json!({
                                    "type": "progress",
                                    "status": info.status.unwrap_or_default(),
                                    "id": info.id.unwrap_or_default(),
                                    "current": info.progress_detail.as_ref().and_then(|detail| detail.current).unwrap_or(0),
                                    "total": info.progress_detail.as_ref().and_then(|detail| detail.total).unwrap_or(0),
                                }));
                            }
                            Some(Err(error)) => {
                                let error = map_bollard(error, false);
                                emit(&events, json!({"type": "closed", "code": error.code, "params": error.params}));
                                return;
                            }
                            None => break,
                        }
                    }
                }
            }
            emit(&events, json!({"type": "closed", "code": "ok", "params": {}}));
        })
    }

    pub async fn networks(&self, endpoint_id: i64) -> Result<Vec<NetworkRow>, DockerError> {
        self.require()?;
        let (docker, _) = self.client(endpoint_id)?;
        let rows = docker.list_networks(None).await.map_err(|error| map_bollard(error, false))?;
        Ok(rows
            .into_iter()
            .map(|item| NetworkRow {
                id: item.id.unwrap_or_default(),
                name: item.name.unwrap_or_default(),
                driver: item.driver.unwrap_or_default(),
                scope: item.scope.unwrap_or_default(),
            })
            .collect())
    }

    pub async fn network_inspect(&self, endpoint_id: i64, name: &str) -> Result<Value, DockerError> {
        self.require()?;
        let name = resource_name(name)?;
        let (docker, _) = self.client(endpoint_id)?;
        let value = docker.inspect_network(name, None).await.map_err(|error| map_bollard(error, false))?;
        serde_json::to_value(value).map_err(|_| DockerError::new("api"))
    }

    pub async fn network_remove(&self, endpoint_id: i64, name: &str) -> Result<(), DockerError> {
        self.require()?;
        let name = resource_name(name)?;
        let (docker, _) = self.client(endpoint_id)?;
        docker.remove_network(name).await.map_err(|error| map_bollard(error, true))
    }

    pub async fn volumes(&self, endpoint_id: i64) -> Result<Vec<VolumeRow>, DockerError> {
        self.require()?;
        let (docker, _) = self.client(endpoint_id)?;
        let listed = docker
            .list_volumes(None::<bollard::query_parameters::ListVolumesOptions>)
            .await
            .map_err(|error| map_bollard(error, false))?;
        Ok(listed
            .volumes
            .unwrap_or_default()
            .into_iter()
            .map(|item| VolumeRow {
                name: item.name,
                driver: item.driver,
                mountpoint: item.mountpoint,
                scope: item.scope.map(|scope| scope.to_string()).unwrap_or_default(),
            })
            .collect())
    }

    pub async fn volume_inspect(&self, endpoint_id: i64, name: &str) -> Result<Value, DockerError> {
        self.require()?;
        let name = resource_name(name)?;
        let (docker, _) = self.client(endpoint_id)?;
        let value = docker.inspect_volume(name).await.map_err(|error| map_bollard(error, false))?;
        serde_json::to_value(value).map_err(|_| DockerError::new("api"))
    }

    pub async fn volume_remove(&self, endpoint_id: i64, name: &str) -> Result<(), DockerError> {
        self.require()?;
        let name = resource_name(name)?;
        let (docker, _) = self.client(endpoint_id)?;
        docker
            .remove_volume(name, None::<bollard::query_parameters::RemoveVolumeOptions>)
            .await
            .map_err(|error| map_bollard(error, true))
    }

    pub async fn compose_projects(&self, endpoint_id: i64) -> Result<Vec<ComposeProject>, DockerError> {
        self.require()?;
        let saved = lock(&self.inner.store).list_compose(endpoint_id)?;
        let recognized = self.recognize_compose(endpoint_id).await?;
        Ok(compose::merge_projects(&saved, &recognized))
    }

    pub fn compose_preview(&self, path: &str) -> Result<ComposePreview, DockerError> {
        self.require()?;
        compose_path(path)?;
        compose::load_compose_file(Path::new(path))
    }

    pub fn compose_import_file(&self, endpoint_id: i64, path: &str) -> Result<(), DockerError> {
        self.require()?;
        let _ = lock(&self.inner.store).get(endpoint_id)?;
        compose_path(path)?;
        let preview = compose::load_compose_file(Path::new(path))?;
        lock(&self.inner.store).upsert_compose(
            endpoint_id,
            &preview.name,
            &preview.config_path,
            &preview.working_dir,
            &preview.services,
            now_millis(),
        )?;
        Ok(())
    }

    pub async fn compose_import_engine(&self, endpoint_id: i64, name: &str) -> Result<(), DockerError> {
        self.require()?;
        let _ = lock(&self.inner.store).get(endpoint_id)?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 128 {
            return Err(DockerError::new("invalid_input"));
        }
        let project = self
            .recognize_compose(endpoint_id)
            .await?
            .into_iter()
            .find(|item| item.name == name)
            .ok_or_else(|| DockerError::new("not_found"))?;
        let config = if project.config_path.is_empty() {
            format!("engine://{name}")
        } else {
            project.config_path
        };
        let services = project
            .services
            .into_iter()
            .map(|service| compose::ComposeService {
                name: service,
                image: String::new(),
                ports: Vec::new(),
            })
            .collect::<Vec<_>>();
        lock(&self.inner.store).upsert_compose(endpoint_id, name, &config, &project.working_dir, &services, now_millis())?;
        Ok(())
    }

    pub fn compose_forget(&self, endpoint_id: i64, id: i64) -> Result<(), DockerError> {
        self.require()?;
        if id <= 0 {
            return Err(DockerError::new("invalid_input"));
        }
        lock(&self.inner.store).delete_compose(endpoint_id, id)
    }

    async fn recognize_compose(&self, endpoint_id: i64) -> Result<Vec<compose::RecognizedProject>, DockerError> {
        let (docker, _) = self.client(endpoint_id)?;
        let options = ListContainersOptionsBuilder::new().all(true).build();
        let rows = docker
            .list_containers(Some(options))
            .await
            .map_err(|error| map_bollard(error, false))?;
        let labeled = rows
            .into_iter()
            .map(|item| LabeledContainer {
                labels: item.labels.unwrap_or_default(),
                running: item.state.as_ref().is_some_and(|state| state.to_string() == "running"),
            })
            .collect::<Vec<_>>();
        Ok(compose::recognize_projects(&labeled))
    }

    fn spawn_task<F, Fut>(&self, endpoint_id: i64, events: Channel<Value>, work: F) -> Result<u64, DockerError>
    where
        F: FnOnce(Channel<Value>, watch::Receiver<bool>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        if lock(&self.inner.tasks).len() >= 8 {
            return Err(DockerError::new("busy"));
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (cancel, receiver) = watch::channel(false);
        let task = tauri::async_runtime::spawn(work(events, receiver));
        lock(&self.inner.tasks).insert(
            id,
            RunningTask {
                endpoint_id,
                cancel,
                task,
            },
        );
        Ok(id)
    }

    fn client(&self, endpoint_id: i64) -> Result<(Docker, Option<String>), DockerError> {
        self.sweep();
        let mut clients = lock(&self.inner.clients);
        let slot = clients.get_mut(&endpoint_id).ok_or_else(|| DockerError::new("disconnected"))?;
        slot.last_used = Instant::now();
        Ok((slot.docker.clone(), slot.socket.clone()))
    }

    fn drop_client(&self, endpoint_id: i64) {
        self.cancel_links(endpoint_id);
        lock(&self.inner.tasks).retain(|_, task| task.endpoint_id != endpoint_id);
        lock(&self.inner.clients).remove(&endpoint_id);
    }

    fn cancel_links(&self, endpoint_id: i64) {
        let bridges = lock(&self.inner.links)
            .iter()
            .filter(|(_, link)| link.endpoint_id == endpoint_id)
            .map(|(_, link)| Arc::clone(&link.bridge))
            .collect::<Vec<_>>();
        for bridge in bridges {
            let _ = bridge.cancel.send(true);
            dial::cancel_prompts(&bridge);
        }
    }

    fn shutdown(&self) {
        let bridges = lock(&self.inner.links)
            .values()
            .map(|link| Arc::clone(&link.bridge))
            .collect::<Vec<_>>();
        for bridge in bridges {
            let _ = bridge.cancel.send(true);
            dial::cancel_prompts(&bridge);
        }
        lock(&self.inner.links).clear();
        lock(&self.inner.tasks).clear();
        lock(&self.inner.clients).clear();
    }

    fn sweep(&self) {
        let active = lock(&self.inner.tasks)
            .values()
            .map(|task| task.endpoint_id)
            .collect::<Vec<_>>();
        lock(&self.inner.clients).retain(|id, slot| active.contains(id) || slot.last_used.elapsed() < IDLE);
    }

    fn touch_menu(&self) {
        let _ = self.inner.app.emit("docker://endpoints", ());
    }

    fn require(&self) -> Result<(), DockerError> {
        if lock(&self.inner.store).enabled()? {
            Ok(())
        } else {
            Err(DockerError::new("disabled"))
        }
    }

    fn ensure_ssh_host(&self, host_id: i64) -> Result<(), DockerError> {
        let Some(ssh) = self.inner.app.try_state::<SshState>() else {
            return Err(DockerError::new("ssh_disabled"));
        };
        if !ssh.tool_enabled()? {
            return Err(DockerError::new("ssh_disabled"));
        }
        ssh.load_host(host_id)?;
        Ok(())
    }

    fn ssh_host_keys(&self) -> Vec<(i64, String, String, u16)> {
        let Some(ssh) = self.inner.app.try_state::<SshState>() else {
            return Vec::new();
        };
        ssh.list()
            .unwrap_or_default()
            .into_iter()
            .map(|host| (host.id, host.username, host.host, host.port))
            .collect()
    }
}

async fn open_and_store(
    inner: &Arc<Inner>,
    endpoint: &Endpoint,
    bridge: &Arc<dial::Bridge>,
    events: &Channel<Value>,
) -> Result<(), DockerError> {
    emit(events, json!({"type": "state", "state": "connecting"}));
    let (docker, proxy, socket) = match endpoint.kind {
        EndpointKind::Local => {
            let path = resolve_local(endpoint.socket_path.as_deref())?;
            let docker = connect_local(&path)?;
            let docker = negotiate(docker).await?;
            (docker, None, Some(path))
        }
        EndpointKind::Ssh => {
            let (docker, proxy) = open_ssh(inner, endpoint, bridge, events).await?;
            (docker, Some(proxy), None)
        }
        EndpointKind::Tls => {
            let docker = connect_tls(endpoint)?;
            let docker = negotiate(docker).await?;
            (docker, None, None)
        }
        EndpointKind::Tcp => {
            let docker = connect_tcp(endpoint)?;
            let docker = negotiate(docker).await?;
            (docker, None, None)
        }
    };
    let summary = read_summary(&docker, socket.clone()).await?;
    {
        let mut clients = lock(&inner.clients);
        if *bridge.cancel.borrow() {
            return Err(DockerError::new("cancelled"));
        }
        clients.insert(
            endpoint.id,
            ClientSlot {
                docker,
                proxy,
                socket,
                last_used: Instant::now(),
            },
        );
    }
    let summary = serde_json::to_value(&summary).unwrap_or_else(|_| json!({}));
    if *bridge.cancel.borrow() {
        lock(&inner.clients).remove(&endpoint.id);
        return Err(DockerError::new("cancelled"));
    }
    emit(events, json!({"type": "ready", "summary": summary}));
    Ok(())
}

async fn open_ssh(
    inner: &Inner,
    endpoint: &Endpoint,
    bridge: &Arc<dial::Bridge>,
    events: &Channel<Value>,
) -> Result<(Docker, Proxy), DockerError> {
    let Some(ssh) = inner.app.try_state::<SshState>() else {
        return Err(DockerError::new("ssh_disabled"));
    };
    if !ssh.tool_enabled()? {
        return Err(DockerError::new("ssh_disabled"));
    }
    let host_id = endpoint.ssh_host_id.ok_or_else(|| DockerError::new("invalid_input"))?;
    let host = ssh.load_host(host_id)?;
    let jumps = ssh.jump_hosts(&host)?;
    let remote = endpoint
        .remote_socket
        .clone()
        .unwrap_or_else(|| "/var/run/docker.sock".to_string());
    let env = ssh.dial_env();
    let dialed = dial::open(&env, &host, &jumps, bridge, events).await?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.map_err(storage_io)?;
    let port = listener.local_addr().map_err(storage_io)?.port();
    let proxy = start_proxy(listener, dialed, remote);
    let docker = Docker::connect_with_http(&format!("http://127.0.0.1:{port}"), API_TIMEOUT, bollard::API_DEFAULT_VERSION)
        .map_err(|error| map_bollard(error, false))?;
    let docker = negotiate(docker).await?;
    Ok((docker, proxy))
}

fn start_proxy(listener: TcpListener, dialed: dial::Dialed, remote_socket: String) -> Proxy {
    let (cancel, cancel_rx) = watch::channel(false);
    let session = Arc::new(AsyncMutex::new(Some(dialed.target)));
    let task_session = Arc::clone(&session);
    let task = tauri::async_runtime::spawn(async move {
        proxy_loop(listener, task_session, remote_socket, cancel_rx).await;
    });
    Proxy {
        cancel,
        task,
        session,
        jumps: dialed.jumps,
    }
}

async fn proxy_loop(
    listener: TcpListener,
    session: Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>,
    remote_socket: String,
    mut cancel: watch::Receiver<bool>,
) {
    if *cancel.borrow() {
        return;
    }
    loop {
        tokio::select! {
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let Ok((socket, _)) = accepted else {
                    continue;
                };
                let session = Arc::clone(&session);
                let remote_socket = remote_socket.clone();
                let mut stop = cancel.clone();
                tauri::async_runtime::spawn(async move {
                    let channel = {
                        let guard = session.lock().await;
                        let Some(handle) = guard.as_ref() else {
                            return;
                        };
                        handle.channel_open_direct_streamlocal(&remote_socket).await.ok()
                    };
                    let Some(channel) = channel else {
                        return;
                    };
                    let mut remote = channel.into_stream();
                    let mut local = socket;
                    tokio::select! {
                        _ = stop.changed() => {}
                        _ = tokio::io::copy_bidirectional(&mut local, &mut remote) => {}
                    }
                });
            }
        }
    }
}

fn resolve_local(path: Option<&str>) -> Result<String, DockerError> {
    if let Some(path) = path {
        return Ok(path.to_string());
    }
    sockets::discover_sockets()
        .into_iter()
        .next()
        .map(|item| item.path)
        .ok_or_else(|| DockerError::new("unreachable"))
}

fn connect_local(path: &str) -> Result<Docker, DockerError> {
    #[cfg(unix)]
    {
        Docker::connect_with_unix(path, API_TIMEOUT, bollard::API_DEFAULT_VERSION).map_err(|error| map_bollard(error, false))
    }
    #[cfg(windows)]
    {
        Docker::connect_with_named_pipe(path, API_TIMEOUT, bollard::API_DEFAULT_VERSION).map_err(|error| map_bollard(error, false))
    }
}

fn connect_tls(endpoint: &Endpoint) -> Result<Docker, DockerError> {
    ensure_tls();
    let host = endpoint.host.as_deref().ok_or_else(|| DockerError::new("invalid_input"))?;
    let port = endpoint.port.ok_or_else(|| DockerError::new("invalid_input"))?;
    let key = endpoint.key_path.as_deref().ok_or_else(|| DockerError::new("invalid_input"))?;
    let cert = endpoint.cert_path.as_deref().ok_or_else(|| DockerError::new("invalid_input"))?;
    let ca = endpoint.ca_path.as_deref().ok_or_else(|| DockerError::new("invalid_input"))?;
    Docker::connect_with_ssl(
        &format!("tcp://{}", host_port(host, port)),
        Path::new(key),
        Path::new(cert),
        Path::new(ca),
        API_TIMEOUT,
        bollard::API_DEFAULT_VERSION,
    )
    .map_err(|error| map_bollard(error, false))
}

fn connect_tcp(endpoint: &Endpoint) -> Result<Docker, DockerError> {
    if !endpoint.insecure {
        return Err(DockerError::new("insecure_required"));
    }
    let host = endpoint.host.as_deref().ok_or_else(|| DockerError::new("invalid_input"))?;
    let port = endpoint.port.ok_or_else(|| DockerError::new("invalid_input"))?;
    Docker::connect_with_http(
        &format!("http://{}", host_port(host, port)),
        API_TIMEOUT,
        bollard::API_DEFAULT_VERSION,
    )
    .map_err(|error| map_bollard(error, false))
}

async fn negotiate(docker: Docker) -> Result<Docker, DockerError> {
    docker.negotiate_version().await.map_err(|error| map_bollard(error, false))
}

async fn read_summary(docker: &Docker, socket: Option<String>) -> Result<EngineSummary, DockerError> {
    let version = docker.version().await.map_err(|error| map_bollard(error, false))?;
    let info = docker.info().await.map_err(|error| map_bollard(error, false))?;
    let name = version
        .components
        .unwrap_or_default()
        .into_iter()
        .find(|item| {
            let name = item.name.to_ascii_lowercase();
            name.contains("docker") || name.contains("podman") || name.contains("engine")
        })
        .map(|item| item.name)
        .unwrap_or_else(|| "Docker".to_string());
    Ok(EngineSummary {
        name,
        hostname: info.name.unwrap_or_default(),
        version: version.version.or(info.server_version).unwrap_or_default(),
        os: version.os.unwrap_or_default(),
        arch: version.arch.unwrap_or_default(),
        cpus: info.ncpu.unwrap_or(0),
        memory: info.mem_total.unwrap_or(0),
        containers: info.containers.unwrap_or(0),
        images: info.images.unwrap_or(0),
        socket,
    })
}

fn emit_log(events: &Channel<Value>, output: &bollard::container::LogOutput) {
    let (stream, bytes) = match output {
        bollard::container::LogOutput::StdErr { message } => ("stderr", message.as_ref()),
        bollard::container::LogOutput::StdOut { message } | bollard::container::LogOutput::Console { message } => ("stdout", message.as_ref()),
        bollard::container::LogOutput::StdIn { .. } => return,
    };
    if bytes.is_empty() {
        return;
    }
    let text = String::from_utf8_lossy(bytes);
    for chunk in text.as_bytes().chunks(32 * 1024) {
        let _ = events.send(json!({
            "type": "data",
            "stream": stream,
            "text": String::from_utf8_lossy(chunk),
        }));
    }
}

fn port_label(port: bollard::models::PortSummary) -> String {
    let protocol = port.typ.map(|kind| kind.to_string()).unwrap_or_else(|| "tcp".to_string());
    match port.public_port {
        Some(public) => format!("{public}->{}/{}", port.private_port, protocol),
        None => format!("{}/{}", port.private_port, protocol),
    }
}

fn host_port(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn resource_name(value: &str) -> Result<&str, DockerError> {
    let value = value.trim();
    if valid_resource_name(value) { Ok(value) } else { Err(DockerError::new("invalid_input")) }
}

fn image_name(value: &str) -> Result<&str, DockerError> {
    let value = value.trim();
    if valid_image_ref(value) { Ok(value) } else { Err(DockerError::new("invalid_input")) }
}

fn signature_saved(endpoint: &Endpoint) -> String {
    signature(endpoint.kind, endpoint.socket_path.as_deref(), endpoint.ssh_host_id, endpoint.remote_socket.as_deref(), endpoint.host.as_deref(), endpoint.port, endpoint.ca_path.as_deref())
}

fn signature_new(endpoint: &NormalizedEndpoint) -> String {
    signature(endpoint.kind, endpoint.socket_path.as_deref(), endpoint.ssh_host_id, endpoint.remote_socket.as_deref(), endpoint.host.as_deref(), endpoint.port, endpoint.ca_path.as_deref())
}

fn signature(kind: EndpointKind, socket: Option<&str>, ssh_host: Option<i64>, remote: Option<&str>, host: Option<&str>, port: Option<u16>, ca: Option<&str>) -> String {
    match kind {
        EndpointKind::Local => format!("local:{}", socket.unwrap_or("")),
        EndpointKind::Ssh => format!("ssh:{}:{}", ssh_host.unwrap_or(0), remote.unwrap_or("")),
        EndpointKind::Tls => format!("tls:{}:{}:{}", host.unwrap_or(""), port.unwrap_or(0), ca.unwrap_or("")),
        EndpointKind::Tcp => format!("tcp:{}:{}", host.unwrap_or(""), port.unwrap_or(0)),
    }
}

fn map_bollard(error: bollard::errors::Error, in_use: bool) -> DockerError {
    use bollard::errors::Error;
    let mapped = match error {
        Error::DockerResponseServerError { status_code, message } => map_status(status_code, &message, in_use),
        Error::DockerStreamError { error } => map_status(0, &error, in_use),
        Error::RequestTimeoutError => DockerError::new("timeout"),
        Error::SocketNotFoundError(_) => DockerError::new("unreachable"),
        Error::CertPathError { .. }
        | Error::CertParseError { .. }
        | Error::CertMultipleKeys { .. }
        | Error::NoNativeCertsError { .. }
        | Error::LoadNativeCertsErrors { .. } => DockerError::new("tls"),
        Error::IOError { err } => classify_io(&err),
        Error::HyperLegacyError { err } => classify_text(&err.to_string()),
        Error::HyperResponseError { err } => classify_text(&err.to_string()),
        other => classify_text(&other.to_string()),
    };
    log_docker("api", mapped.code);
    mapped
}

fn map_status(status: u16, message: &str, in_use: bool) -> DockerError {
    let lower = message.to_ascii_lowercase();
    if status == 401
        || lower.contains("unauthorized")
        || lower.contains("authentication required")
        || lower.contains("pull access denied")
    {
        return DockerError::new("registry_auth");
    }
    if status == 404 {
        return DockerError::new("not_found");
    }
    if status == 409 {
        return DockerError::new(if in_use { "in_use" } else { "conflict" });
    }
    if status == 403 {
        return DockerError::new("permission_denied");
    }
    DockerError::with_params("api", serde_json::json!({ "status": status }))
}

fn classify_io(error: &std::io::Error) -> DockerError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        DockerError::new("permission_denied")
    } else if error.kind() == std::io::ErrorKind::TimedOut {
        DockerError::new("timeout")
    } else if error.kind() == std::io::ErrorKind::NotFound {
        DockerError::new("unreachable")
    } else {
        classify_text(&error.to_string())
    }
}

fn classify_text(text: &str) -> DockerError {
    let lower = text.to_ascii_lowercase();
    if lower.contains("permission denied") || lower.contains("access is denied") || lower.contains("operation not permitted") {
        DockerError::new("permission_denied")
    } else if lower.contains("timed out") || lower.contains("timeout") {
        DockerError::new("timeout")
    } else {
        DockerError::new("unreachable")
    }
}

fn ensure_tls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn emit(events: &Channel<Value>, value: Value) {
    let _ = events.send(value);
}

fn storage_io(error: impl std::fmt::Display) -> DockerError {
    eprintln!("docker io: {error}");
    DockerError::new("unreachable")
}

fn tighten(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn compose_path(path: &str) -> Result<(), DockerError> {
    if path.is_empty() || path.len() > 1024 || path.contains('\0') {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(())
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}
