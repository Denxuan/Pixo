use std::collections::HashMap;
use std::io::Read;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bollard::Docker;
use bollard::models::{
    ContainerCreateBody, ContainerStatsResponse, EndpointSettings, HostConfig, Mount,
    MountBindOptions, MountType, Network, NetworkingConfig, PortBinding, RestartPolicy,
    RestartPolicyNameEnum,
};
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptionsBuilder, ListContainersOptionsBuilder,
    ListImagesOptionsBuilder, LogsOptionsBuilder, RemoveContainerOptionsBuilder,
    RemoveImageOptionsBuilder, RestartContainerOptionsBuilder, StatsOptionsBuilder,
    StopContainerOptionsBuilder,
};
use futures_util::{StreamExt, stream};
use russh::client;
use russh::{ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileType, OpenFlags, StatusCode};
use serde_json::{Value, json};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::{mpsc, watch};

use crate::ssh::SshState;
use crate::ssh::dial::{self, ClientHandler};

use super::compose::{self, ComposePreview, ComposeProject, LabeledContainer};
use super::context::{self, ContextPreview};
use super::error::{DockerError, log_docker};
use super::hostdir;
use super::model::{
    ContainerRow, ContainerUsage, DockerStatus, Endpoint, EndpointDraft, EndpointKind,
    EngineSummary, HostDirEntry, HostDirListing, ImageRow, NetworkRow, NormalizedEndpoint,
    normalize, normalize_run, split_command, valid_image_ref, valid_resource_name,
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
    execs: Mutex<HashMap<u64, ExecSlot>>,
    next_id: AtomicU64,
}

struct ExecSlot {
    endpoint_id: i64,
    cancel: watch::Sender<bool>,
    input: mpsc::UnboundedSender<Vec<u8>>,
    resize: mpsc::UnboundedSender<(u16, u16)>,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl Drop for ExecSlot {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        self.task.abort();
    }
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
            execs: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }),
    })
}

impl DockerState {
    pub fn status(&self) -> Result<DockerStatus, DockerError> {
        let store = lock(&self.inner.store);
        Ok(DockerStatus {
            enabled: store.enabled()?,
            image_updates: store.image_updates()?,
        })
    }

    pub fn link_count(&self) -> usize {
        lock(&self.inner.links).len()
    }

    pub fn stored(&self) -> Result<Vec<Endpoint>, DockerError> {
        lock(&self.inner.store).list()
    }

    pub fn endpoint_names_for_host(&self, host_id: i64) -> Vec<String> {
        self.stored()
            .unwrap_or_default()
            .into_iter()
            .filter(|endpoint| endpoint.ssh_host_id == Some(host_id))
            .map(|endpoint| endpoint.name)
            .collect()
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

    pub fn set_image_updates(&self, enabled: bool) -> Result<DockerStatus, DockerError> {
        lock(&self.inner.store).set_image_updates(enabled)?;
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
            if existing
                .iter()
                .any(|item| signature_saved(item) == signature_new(&endpoint))
            {
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
        self.note_links();
        let inner = Arc::clone(&self.inner);
        tauri::async_runtime::spawn(async move {
            let result = open_and_store(&inner, &endpoint, &bridge, &events).await;
            lock(&inner.links).remove(&id);
            let _ = inner.app.emit("docker://links", ());
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
        let link = links
            .get(&link_id)
            .ok_or_else(|| DockerError::new("not_found"))?;
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
        let link = links
            .get(&link_id)
            .ok_or_else(|| DockerError::new("not_found"))?;
        dial::answer_prompt(&link.bridge, prompt_id, secret, remember, cancel)?;
        Ok(())
    }

    pub async fn summary(&self, endpoint_id: i64) -> Result<EngineSummary, DockerError> {
        self.require()?;
        let (docker, socket) = self.client(endpoint_id)?;
        read_summary(&docker, socket).await
    }

    pub async fn mirrors(&self, endpoint_id: i64) -> Result<Vec<String>, DockerError> {
        self.require()?;
        let (docker, _) = self.client(endpoint_id)?;
        let info = docker
            .info()
            .await
            .map_err(|error| map_bollard(error, false))?;
        Ok(configured_mirrors(&info))
    }

    pub async fn set_mirrors(
        &self,
        endpoint_id: i64,
        requested: Vec<String>,
    ) -> Result<(), DockerError> {
        self.require()?;
        let mirrors = super::mirrors::normalize_mirrors(&requested)?;
        let endpoint = lock(&self.inner.store).get(endpoint_id)?;
        let plan = {
            let (docker, socket) = self.client(endpoint_id)?;
            let info = docker
                .info()
                .await
                .map_err(|error| map_bollard(error, false))?;
            let version = docker
                .version()
                .await
                .map_err(|error| map_bollard(error, false))?;
            if podman_engine(&version) {
                return Err(DockerError::new("mirror_unsupported"));
            }
            super::mirrors::daemon_plan(
                endpoint.kind == EndpointKind::Local,
                cfg!(target_os = "macos"),
                info.os_type.as_deref().unwrap_or(""),
                info.operating_system.as_deref().unwrap_or(""),
                info.security_options.as_deref().unwrap_or(&[]),
                socket.as_deref().unwrap_or(""),
            )?
        };
        let current = self
            .engine_shell(endpoint_id, &endpoint, &super::mirrors::read_script(&plan)?)
            .await?;
        let existing = super::mirrors::command_stdout(&current).map_err(|error| {
            log_docker("mirrors", error.code);
            error
        })?;
        let merged = super::mirrors::merge_daemon(&existing, &mirrors)?;
        let script =
            super::mirrors::write_script(&plan, &super::mirrors::encode_base64(merged.as_bytes()))?;
        let wrote = self.engine_shell(endpoint_id, &endpoint, &script).await?;
        if let Err(error) = super::mirrors::map_shell(wrote.status, &wrote.stderr) {
            log_docker("mirrors", error.code);
            return Err(error);
        }
        self.drop_client(endpoint_id);
        Ok(())
    }

    async fn engine_shell(
        &self,
        endpoint_id: i64,
        endpoint: &Endpoint,
        script: &str,
    ) -> Result<super::mirrors::ShellOutput, DockerError> {
        match endpoint.kind {
            EndpointKind::Local if cfg!(windows) => Err(DockerError::new("mirror_unsupported")),
            EndpointKind::Local => local_shell(script.to_string()).await,
            EndpointKind::Ssh => {
                let session = self.ssh_session(endpoint_id)?;
                remote_shell(&session, script).await
            }
        }
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
                state: item
                    .state
                    .map(|state| state.to_string())
                    .unwrap_or_default(),
                status: item.status.unwrap_or_default(),
                ports: item
                    .ports
                    .unwrap_or_default()
                    .into_iter()
                    .map(port_label)
                    .collect(),
                created: item.created.unwrap_or(0),
            })
            .collect())
    }

    pub async fn container_action(
        &self,
        endpoint_id: i64,
        id: &str,
        action: &str,
        force: bool,
    ) -> Result<(), DockerError> {
        self.require()?;
        let id = resource_name(id)?;
        let (docker, _) = self.client(endpoint_id)?;
        let result = match action {
            "start" => docker.start_container(id, None).await,
            "stop" => {
                docker
                    .stop_container(id, Some(StopContainerOptionsBuilder::new().t(10).build()))
                    .await
            }
            "restart" => {
                docker
                    .restart_container(
                        id,
                        Some(RestartContainerOptionsBuilder::new().t(10).build()),
                    )
                    .await
            }
            "remove" => {
                self.keep_compose_of_container(endpoint_id, id).await?;
                docker
                    .remove_container(
                        id,
                        Some(RemoveContainerOptionsBuilder::new().force(force).build()),
                    )
                    .await
            }
            _ => return Err(DockerError::new("invalid_input")),
        };
        result.map_err(|error| map_bollard(error, false))
    }

    pub async fn container_inspect(
        &self,
        endpoint_id: i64,
        id: &str,
    ) -> Result<Value, DockerError> {
        self.require()?;
        let id = resource_name(id)?;
        let (docker, _) = self.client(endpoint_id)?;
        let value = docker
            .inspect_container(id, None)
            .await
            .map_err(|error| map_bollard(error, false))?;
        serde_json::to_value(value).map_err(|_| DockerError::new("api"))
    }

    pub fn container_logs(
        &self,
        endpoint_id: i64,
        id: &str,
        events: Channel<Value>,
    ) -> Result<u64, DockerError> {
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
        let rows = docker
            .list_images(Some(options))
            .await
            .map_err(|error| map_bollard(error, false))?;
        Ok(rows
            .into_iter()
            .map(|item| ImageRow {
                id: item.id,
                tags: item
                    .repo_tags
                    .into_iter()
                    .filter(|tag| tag != "<none>:<none>")
                    .collect(),
                size: item.size,
                created: item.created,
            })
            .collect())
    }

    pub async fn image_inspect(&self, endpoint_id: i64, name: &str) -> Result<Value, DockerError> {
        self.require()?;
        let name = image_name(name)?;
        let (docker, _) = self.client(endpoint_id)?;
        let value = docker
            .inspect_image(name)
            .await
            .map_err(|error| map_bollard(error, false))?;
        serde_json::to_value(value).map_err(|_| DockerError::new("api"))
    }

    pub async fn image_updates(
        &self,
        endpoint_id: i64,
        events: Channel<Value>,
    ) -> Result<(), DockerError> {
        self.require()?;
        if !lock(&self.inner.store).image_updates()? {
            return Err(DockerError::new("image_updates_off"));
        }
        let (docker, _) = self.client(endpoint_id)?;
        let options = ListImagesOptionsBuilder::new().all(false).build();
        let images = docker
            .list_images(Some(options))
            .await
            .map_err(|error| map_bollard(error, false))?;
        let mut jobs = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for image in images {
            if jobs.len() == 32 {
                break;
            }
            let digests = image.repo_digests;
            for tag in image.repo_tags {
                if jobs.len() == 32 {
                    break;
                }
                if !valid_image_ref(&tag)
                    || !super::updates::checkable_reference(&tag)
                    || !seen.insert(tag.clone())
                {
                    continue;
                }
                let credentials = super::registry::credentials(&tag, None, None)
                    .ok()
                    .flatten();
                jobs.push(RegistryCheck {
                    tag,
                    digests: digests.clone(),
                    credentials,
                });
            }
        }
        let mut pending = stream::iter(jobs)
            .map(|job| {
                let docker = docker.clone();
                async move {
                    let state = registry_digest_state(&docker, &job).await;
                    super::updates::ImageUpdate {
                        reference: job.tag,
                        state: state.to_string(),
                    }
                }
            })
            .buffer_unordered(4);
        while let Some(update) = pending.next().await {
            emit(
                &events,
                json!({
                    "type": "update",
                    "reference": update.reference,
                    "state": update.state,
                }),
            );
        }
        Ok(())
    }

    pub async fn image_users(
        &self,
        endpoint_id: i64,
        reference: &str,
    ) -> Result<Vec<String>, DockerError> {
        self.require()?;
        if !lock(&self.inner.store).image_updates()? {
            return Err(DockerError::new("image_updates_off"));
        }
        let reference = image_name(reference)?.to_string();
        let (docker, _) = self.client(endpoint_id)?;
        let inspected = docker
            .inspect_image(&reference)
            .await
            .map_err(|error| map_bollard(error, false))?;
        let image_id = inspected.id.unwrap_or_default();
        let options = ListContainersOptionsBuilder::new().all(true).build();
        let containers = docker
            .list_containers(Some(options))
            .await
            .map_err(|error| map_bollard(error, false))?;
        let uses = containers
            .iter()
            .map(|item| super::updates::ImageUse {
                name: item
                    .names
                    .as_ref()
                    .and_then(|names| names.first())
                    .map(|name| name.trim_start_matches('/').to_string())
                    .unwrap_or_default(),
                image: item.image.clone().unwrap_or_default(),
                image_id: item.image_id.clone().unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        Ok(super::updates::containers_using(
            &reference, &image_id, &uses,
        ))
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

    pub async fn image_run(
        &self,
        endpoint_id: i64,
        request: super::model::ContainerRunRequest,
    ) -> Result<String, DockerError> {
        self.require()?;
        let run = normalize_run(request)?;
        let (docker, _) = self.client(endpoint_id)?;
        let (exposed, bindings) = port_config(&run.ports)?;
        let restart = run.restart.as_deref().and_then(restart_policy);
        let mounts = run_mounts(&run.mounts);
        let has_mounts = !mounts.is_empty();
        let network = run.network.clone();
        let host_config =
            (bindings.is_some() || network.is_some() || restart.is_some() || has_mounts).then(
                || HostConfig {
                    port_bindings: bindings,
                    network_mode: network.clone(),
                    restart_policy: restart,
                    mounts: has_mounts.then_some(mounts),
                    ..HostConfig::default()
                },
            );
        let networking_config = network
            .filter(|name| !matches!(name.as_str(), "host" | "none" | "bridge"))
            .map(|name| {
                let mut endpoints = HashMap::new();
                endpoints.insert(name, EndpointSettings::default());
                NetworkingConfig {
                    endpoints_config: Some(endpoints),
                }
            });
        let created = docker
            .create_container(
                Some(CreateContainerOptions {
                    name: run.name,
                    platform: String::new(),
                }),
                ContainerCreateBody {
                    image: Some(run.image),
                    cmd: (!run.command.is_empty()).then_some(run.command),
                    env: (!run.env.is_empty()).then_some(run.env),
                    working_dir: run.workdir,
                    exposed_ports: exposed,
                    host_config,
                    networking_config,
                    ..ContainerCreateBody::default()
                },
            )
            .await
            .map_err(map_create_error)?;
        if let Err(error) = docker
            .start_container(
                &created.id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
        {
            let _ = docker
                .remove_container(
                    &created.id,
                    Some(RemoveContainerOptionsBuilder::new().force(true).build()),
                )
                .await;
            return Err(map_run_error(error));
        }
        Ok(created.id)
    }

    pub async fn container_stats(
        &self,
        endpoint_id: i64,
        id: &str,
    ) -> Result<ContainerUsage, DockerError> {
        self.require()?;
        let id = resource_name(id)?;
        let (docker, _) = self.client(endpoint_id)?;
        let options = StatsOptionsBuilder::new().stream(false).build();
        let sample = docker
            .stats(id, Some(options))
            .next()
            .await
            .ok_or_else(|| DockerError::new("not_found"))?
            .map_err(|error| map_bollard(error, false))?;
        Ok(usage_of(&sample))
    }

    pub fn image_pull(
        &self,
        endpoint_id: i64,
        reference: &str,
        username: Option<String>,
        password: Option<String>,
        remember: bool,
        events: Channel<Value>,
    ) -> Result<u64, DockerError> {
        self.require()?;
        let reference = image_name(reference)?.to_string();
        let username = username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let password = password.filter(|value| !value.is_empty());
        if remember {
            if let (Some(username), Some(password)) = (username, password.as_deref()) {
                super::registry::save(
                    &super::registry::registry_host(&reference),
                    username,
                    password,
                )?;
            }
        }
        let credentials = super::registry::credentials(&reference, username, password.as_deref())?;
        let (docker, _) = self.client(endpoint_id)?;
        self.spawn_task(endpoint_id, events, move |events, mut cancel| async move {
            let previous = docker
                .inspect_image(&reference)
                .await
                .ok()
                .and_then(|image| image.id);
            let options = CreateImageOptionsBuilder::new().from_image(&reference).build();
            let mut stream = docker.create_image(Some(options), None, credentials);
            let mut cancelled = false;
            loop {
                tokio::select! {
                    _ = cancel.wait_for(|stopped| *stopped) => {
                        cancelled = true;
                        break;
                    }
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
            if !cancelled {
                if let Some(previous) = previous.as_deref() {
                    remove_replaced_image(&docker, &reference, previous).await;
                }
            }
            emit(&events, json!({"type": "closed", "code": "ok", "params": {}}));
        })
    }

    pub async fn exec_open(
        &self,
        endpoint_id: i64,
        container_id: &str,
        cols: u32,
        rows: u32,
        command: &str,
        output: Channel<InvokeResponseBody>,
        events: Channel<Value>,
    ) -> Result<u64, DockerError> {
        self.require()?;
        let container_id = resource_name(container_id)?.to_string();
        let command = exec_command(command)?;
        if lock(&self.inner.execs).len() >= 4 {
            return Err(DockerError::new("exec_limit"));
        }
        let (docker, _) = self.client(endpoint_id)?;
        let (cols, rows) = (cols.clamp(1, 1000) as u16, rows.clamp(1, 400) as u16);
        let created = docker
            .create_exec(
                &container_id,
                bollard::exec::CreateExecOptions {
                    attach_stdin: Some(true),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    tty: Some(true),
                    cmd: Some(command),
                    ..Default::default()
                },
            )
            .await
            .map_err(|error| map_bollard(error, false))?;
        let started = docker
            .start_exec(
                &created.id,
                Some(bollard::exec::StartExecOptions {
                    detach: false,
                    tty: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err(|error| map_bollard(error, false))?;
        let bollard::exec::StartExecResults::Attached {
            output: stream,
            input,
        } = started
        else {
            return Err(DockerError::new("exec_failed"));
        };
        let exec_id = created.id;
        let (cancel, cancel_rx) = watch::channel(false);
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let (resize_tx, resize_rx) = mpsc::unbounded_channel();
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let docker_for_task = docker.clone();
        let task = tauri::async_runtime::spawn(async move {
            run_exec(
                docker_for_task,
                exec_id,
                (cols, rows),
                stream,
                input,
                output,
                events,
                input_rx,
                resize_rx,
                cancel_rx,
            )
            .await;
        });
        lock(&self.inner.execs).insert(
            id,
            ExecSlot {
                endpoint_id,
                cancel,
                input: input_tx,
                resize: resize_tx,
                task,
            },
        );
        Ok(id)
    }

    pub fn exec_write(&self, id: u64, data: Vec<u8>) -> Result<(), DockerError> {
        self.require()?;
        if data.len() > 1024 * 1024 {
            return Err(DockerError::new("invalid_input"));
        }
        let execs = lock(&self.inner.execs);
        let slot = execs
            .get(&id)
            .ok_or_else(|| DockerError::new("not_found"))?;
        slot.input
            .send(data)
            .map_err(|_| DockerError::new("disconnected"))
    }

    pub fn exec_resize(&self, id: u64, cols: u32, rows: u32) -> Result<(), DockerError> {
        self.require()?;
        let execs = lock(&self.inner.execs);
        let slot = execs
            .get(&id)
            .ok_or_else(|| DockerError::new("not_found"))?;
        let size = (cols.clamp(1, 1000) as u16, rows.clamp(1, 400) as u16);
        slot.resize
            .send(size)
            .map_err(|_| DockerError::new("disconnected"))
    }

    pub fn exec_close(&self, id: u64) -> Result<(), DockerError> {
        self.require()?;
        lock(&self.inner.execs)
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DockerError::new("not_found"))
    }

    pub async fn networks(&self, endpoint_id: i64) -> Result<Vec<NetworkRow>, DockerError> {
        self.require()?;
        let (docker, _) = self.client(endpoint_id)?;
        let rows = docker
            .list_networks(None)
            .await
            .map_err(|error| map_bollard(error, false))?;
        Ok(rows.into_iter().map(network_row).collect())
    }

    pub async fn network_inspect(
        &self,
        endpoint_id: i64,
        name: &str,
    ) -> Result<Value, DockerError> {
        self.require()?;
        let name = resource_name(name)?;
        let (docker, _) = self.client(endpoint_id)?;
        let value = docker
            .inspect_network(name, None)
            .await
            .map_err(|error| map_bollard(error, false))?;
        serde_json::to_value(value).map_err(|_| DockerError::new("api"))
    }

    pub async fn network_remove(&self, endpoint_id: i64, name: &str) -> Result<(), DockerError> {
        self.require()?;
        let name = resource_name(name)?;
        let (docker, _) = self.client(endpoint_id)?;
        docker
            .remove_network(name)
            .await
            .map_err(|error| map_bollard(error, true))
    }

    pub async fn list_host_dirs(
        &self,
        endpoint_id: i64,
        path: &str,
    ) -> Result<HostDirListing, DockerError> {
        self.require()?;
        if path.len() > 1024 || path.contains('\0') {
            return Err(DockerError::new("invalid_input"));
        }
        let endpoint = lock(&self.inner.store).get(endpoint_id)?;
        match endpoint.kind {
            EndpointKind::Local => hostdir::list_local(path),
            EndpointKind::Ssh => {
                let session = self.ssh_session(endpoint_id)?;
                list_remote_host_dirs(&session, path).await
            }
        }
    }

    pub async fn compose_projects(
        &self,
        endpoint_id: i64,
    ) -> Result<Vec<ComposeProject>, DockerError> {
        self.require()?;
        let saved = lock(&self.inner.store).list_compose(endpoint_id)?;
        let hidden = lock(&self.inner.store).list_hidden(endpoint_id)?;
        let recognized = self
            .recognize_compose(endpoint_id)
            .await?
            .into_iter()
            .filter(|project| !compose::is_hidden(&hidden, &project.name, &project.config_path))
            .collect::<Vec<_>>();
        Ok(compose::merge_projects(&saved, &recognized))
    }

    pub fn compose_preview(&self, path: &str) -> Result<ComposePreview, DockerError> {
        self.require()?;
        compose_path(path)?;
        compose::load_compose_file(Path::new(path))
    }

    pub async fn compose_create(
        &self,
        endpoint_id: i64,
        draft: compose::ComposeProjectRequest,
    ) -> Result<(), DockerError> {
        self.require()?;
        let endpoint = lock(&self.inner.store).get(endpoint_id)?;
        let name = draft.name.trim();
        if !compose::valid_project_name(name) {
            return Err(DockerError::new("invalid_input"));
        }
        let directory = host_directory_for(&endpoint, draft.directory.trim())?;
        let text = project_text(&draft)?;
        let config_path = match endpoint.kind {
            EndpointKind::Local => compose::install_host_compose(Path::new(&directory), &text)?
                .to_string_lossy()
                .into_owned(),
            EndpointKind::Ssh => {
                let session = self.ssh_session(endpoint_id)?;
                write_remote_compose(&session, &directory, &text).await?
            }
        };
        let preview = compose::parse_compose(&text, Path::new("docker-compose.yml"))?;
        lock(&self.inner.store).upsert_compose(
            endpoint_id,
            name,
            &config_path,
            &directory,
            &preview.services,
            now_millis(),
        )?;
        if draft.start {
            match endpoint.kind {
                EndpointKind::Local => start_compose(&endpoint, name, Path::new(&config_path))?,
                EndpointKind::Ssh => {
                    let session = self.ssh_session(endpoint_id)?;
                    start_remote_compose(&session, name, &directory, &config_path).await?;
                }
            }
        }
        Ok(())
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

    pub async fn compose_import_engine(
        &self,
        endpoint_id: i64,
        name: &str,
    ) -> Result<(), DockerError> {
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
        lock(&self.inner.store).upsert_compose(
            endpoint_id,
            name,
            &config,
            &project.working_dir,
            &services,
            now_millis(),
        )?;
        Ok(())
    }

    pub fn compose_forget(&self, endpoint_id: i64, id: i64) -> Result<(), DockerError> {
        self.require()?;
        if id <= 0 {
            return Err(DockerError::new("invalid_input"));
        }
        lock(&self.inner.store).forget_compose(endpoint_id, id)
    }

    pub async fn compose_action(
        &self,
        endpoint_id: i64,
        name: &str,
        action: &str,
    ) -> Result<(), DockerError> {
        let action = compose::ComposeAction::parse(action)?;
        if matches!(action, compose::ComposeAction::Logs) {
            return Err(DockerError::new("invalid_input"));
        }
        let (endpoint, project, location, _) = self.compose_target(endpoint_id, name).await?;
        self.run_compose(&endpoint, endpoint_id, action, &project, &location)
            .await
    }

    pub async fn compose_remove(&self, endpoint_id: i64, name: &str) -> Result<(), DockerError> {
        self.require()?;
        let name = name.trim();
        if !compose::valid_project_name(name) {
            return Err(DockerError::new("invalid_input"));
        }
        let saved = lock(&self.inner.store).list_compose(endpoint_id)?;
        let recognized = self.recognize_compose(endpoint_id).await?;
        let project = compose::merge_projects(&saved, &recognized)
            .into_iter()
            .find(|item| item.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| DockerError::new("not_found"))?;
        lock(&self.inner.store).hide_compose(
            endpoint_id,
            project.id,
            &project.name,
            &project.config_path,
        )
    }

    pub async fn compose_file(&self, endpoint_id: i64, name: &str) -> Result<String, DockerError> {
        let (endpoint, _, location, _) = self.compose_target(endpoint_id, name).await?;
        match endpoint.kind {
            EndpointKind::Local => read_local_compose(&location.file),
            EndpointKind::Ssh => {
                let session = self.ssh_session(endpoint_id)?;
                read_remote_compose(&session, &location.file).await
            }
        }
    }

    pub async fn compose_logs(
        &self,
        endpoint_id: i64,
        name: &str,
        events: Channel<Value>,
    ) -> Result<u64, DockerError> {
        let (endpoint, project, location, _) = self.compose_target(endpoint_id, name).await?;
        match endpoint.kind {
            EndpointKind::Local => {
                self.spawn_task(endpoint_id, events, move |events, cancel| async move {
                    stream_local_compose_logs(endpoint, project, location, events, cancel).await;
                })
            }
            EndpointKind::Ssh => {
                let session = self.ssh_session(endpoint_id)?;
                let command = compose::compose_remote_command(
                    compose::ComposeAction::Logs,
                    &project,
                    &location,
                )?;
                self.spawn_task(endpoint_id, events, move |events, cancel| async move {
                    stream_remote_compose_logs(session, command, events, cancel).await;
                })
            }
        }
    }

    async fn compose_target(
        &self,
        endpoint_id: i64,
        name: &str,
    ) -> Result<(Endpoint, String, compose::ComposeLocation, Option<i64>), DockerError> {
        self.require()?;
        let name = name.trim();
        if !compose::valid_project_name(name) {
            return Err(DockerError::new("invalid_input"));
        }
        let endpoint = lock(&self.inner.store).get(endpoint_id)?;
        let saved = lock(&self.inner.store).list_compose(endpoint_id)?;
        let recognized = self.recognize_compose(endpoint_id).await?;
        let project = compose::merge_projects(&saved, &recognized)
            .into_iter()
            .find(|item| item.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| DockerError::new("not_found"))?;
        let location = compose::compose_location(
            &project.config_path,
            &project.working_dir,
            path_style(&endpoint),
        )?;
        Ok((endpoint, project.name, location, project.id))
    }

    async fn run_compose(
        &self,
        endpoint: &Endpoint,
        endpoint_id: i64,
        action: compose::ComposeAction,
        project: &str,
        location: &compose::ComposeLocation,
    ) -> Result<(), DockerError> {
        match endpoint.kind {
            EndpointKind::Local => {
                run_local_compose(endpoint, location, project, action, action.failure_code())
            }
            EndpointKind::Ssh => {
                let session = self.ssh_session(endpoint_id)?;
                let command = compose::compose_remote_command(action, project, location)?;
                run_remote_compose(&session, command, action.failure_code()).await
            }
        }
    }

    async fn keep_compose_of_container(
        &self,
        endpoint_id: i64,
        id: &str,
    ) -> Result<(), DockerError> {
        let (docker, _) = self.client(endpoint_id)?;
        let inspected = docker
            .inspect_container(id, None)
            .await
            .map_err(|error| map_bollard(error, false))?;
        let config = inspected.config.unwrap_or_default();
        let labels = config.labels.unwrap_or_default();
        let service = labels
            .get("com.docker.compose.service")
            .map(|value| value.trim().to_string())
            .unwrap_or_default();
        let image = config.image.unwrap_or_default();
        let recognized = self.recognize_compose(endpoint_id).await?;
        let saved = lock(&self.inner.store).list_compose(endpoint_id)?;
        let hidden = lock(&self.inner.store).list_hidden(endpoint_id)?;
        let Some(project) = compose::project_to_keep(&labels, &recognized, &saved, &hidden) else {
            return Ok(());
        };
        let services = project
            .services
            .into_iter()
            .map(|name| compose::ComposeService {
                image: if !service.is_empty() && name.eq_ignore_ascii_case(&service) {
                    image.clone()
                } else {
                    String::new()
                },
                name,
                ports: Vec::new(),
            })
            .collect::<Vec<_>>();
        lock(&self.inner.store).upsert_compose(
            endpoint_id,
            &project.name,
            &project.config_path,
            &project.working_dir,
            &services,
            now_millis(),
        )?;
        Ok(())
    }

    async fn recognize_compose(
        &self,
        endpoint_id: i64,
    ) -> Result<Vec<compose::RecognizedProject>, DockerError> {
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
                running: item
                    .state
                    .as_ref()
                    .is_some_and(|state| state.to_string() == "running"),
            })
            .collect::<Vec<_>>();
        Ok(compose::recognize_projects(&labeled))
    }

    fn spawn_task<F, Fut>(
        &self,
        endpoint_id: i64,
        events: Channel<Value>,
        work: F,
    ) -> Result<u64, DockerError>
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
        {
            let mut clients = lock(&self.inner.clients);
            if let Some(slot) = clients.get_mut(&endpoint_id) {
                slot.last_used = Instant::now();
            }
        }
        self.sweep();
        let clients = lock(&self.inner.clients);
        let slot = clients
            .get(&endpoint_id)
            .ok_or_else(|| DockerError::new("disconnected"))?;
        Ok((slot.docker.clone(), slot.socket.clone()))
    }

    fn ssh_session(
        &self,
        endpoint_id: i64,
    ) -> Result<Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>, DockerError> {
        let _ = self.client(endpoint_id)?;
        let clients = lock(&self.inner.clients);
        let slot = clients
            .get(&endpoint_id)
            .ok_or_else(|| DockerError::new("disconnected"))?;
        slot.proxy
            .as_ref()
            .map(|proxy| Arc::clone(&proxy.session))
            .ok_or_else(|| DockerError::new("disconnected"))
    }

    pub async fn renew(&self, endpoint_id: i64) -> Result<(), DockerError> {
        self.require()?;
        let (docker, _) = self.client(endpoint_id)?;
        match docker.ping().await {
            Ok(_) => Ok(()),
            Err(error) => {
                let mapped = map_bollard(error, false);
                if mapped.code == "unreachable" || mapped.code == "timeout" {
                    self.drop_client(endpoint_id);
                }
                Err(mapped)
            }
        }
    }

    fn drop_client(&self, endpoint_id: i64) {
        self.cancel_links(endpoint_id);
        lock(&self.inner.tasks).retain(|_, task| task.endpoint_id != endpoint_id);
        lock(&self.inner.execs).retain(|_, exec| exec.endpoint_id != endpoint_id);
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
        self.note_links();
        lock(&self.inner.tasks).clear();
        lock(&self.inner.execs).clear();
        lock(&self.inner.clients).clear();
    }

    fn sweep(&self) {
        let active = lock(&self.inner.tasks)
            .values()
            .map(|task| task.endpoint_id)
            .collect::<Vec<_>>();
        lock(&self.inner.clients)
            .retain(|id, slot| active.contains(id) || slot.last_used.elapsed() < IDLE);
    }

    fn touch_menu(&self) {
        let _ = self.inner.app.emit("docker://endpoints", ());
    }

    fn note_links(&self) {
        let _ = self.inner.app.emit("docker://links", ());
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
    let host_id = endpoint
        .ssh_host_id
        .ok_or_else(|| DockerError::new("invalid_input"))?;
    let host = ssh.load_host(host_id)?;
    let jumps = ssh.jump_hosts(&host)?;
    let remote = endpoint
        .remote_socket
        .clone()
        .unwrap_or_else(|| "/var/run/docker.sock".to_string());
    let env = ssh.dial_env();
    let dialed = dial::open(&env, &host, &jumps, bridge, events).await?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(storage_io)?;
    let port = listener.local_addr().map_err(storage_io)?.port();
    let proxy = start_proxy(listener, dialed, remote);
    let docker = Docker::connect_with_http(
        &format!("http://127.0.0.1:{port}"),
        API_TIMEOUT,
        bollard::API_DEFAULT_VERSION,
    )
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
        Docker::connect_with_unix(path, API_TIMEOUT, bollard::API_DEFAULT_VERSION)
            .map_err(|error| map_bollard(error, false))
    }
    #[cfg(windows)]
    {
        Docker::connect_with_named_pipe(path, API_TIMEOUT, bollard::API_DEFAULT_VERSION)
            .map_err(|error| map_bollard(error, false))
    }
}

async fn negotiate(docker: Docker) -> Result<Docker, DockerError> {
    docker
        .negotiate_version()
        .await
        .map_err(|error| map_bollard(error, false))
}

async fn read_summary(
    docker: &Docker,
    socket: Option<String>,
) -> Result<EngineSummary, DockerError> {
    let version = docker
        .version()
        .await
        .map_err(|error| map_bollard(error, false))?;
    let info = docker
        .info()
        .await
        .map_err(|error| map_bollard(error, false))?;
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
        bollard::container::LogOutput::StdOut { message }
        | bollard::container::LogOutput::Console { message } => ("stdout", message.as_ref()),
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
    let protocol = port
        .typ
        .map(|kind| kind.to_string())
        .unwrap_or_else(|| "tcp".to_string());
    match port.public_port {
        Some(public) => format!("{public}->{}/{}", port.private_port, protocol),
        None => format!("{}/{}", port.private_port, protocol),
    }
}

fn resource_name(value: &str) -> Result<&str, DockerError> {
    let value = value.trim();
    if valid_resource_name(value) {
        Ok(value)
    } else {
        Err(DockerError::new("invalid_input"))
    }
}

fn restart_policy(name: &str) -> Option<RestartPolicy> {
    let policy = match name {
        "always" => RestartPolicyNameEnum::ALWAYS,
        "unless-stopped" => RestartPolicyNameEnum::UNLESS_STOPPED,
        "on-failure" => RestartPolicyNameEnum::ON_FAILURE,
        _ => return None,
    };
    Some(RestartPolicy {
        name: Some(policy),
        maximum_retry_count: (name == "on-failure").then_some(3),
    })
}

fn run_mounts(mounts: &[super::model::RunMount]) -> Vec<Mount> {
    mounts
        .iter()
        .map(|item| Mount {
            target: Some(item.target.clone()),
            source: Some(item.source.clone()),
            typ: Some(if item.bind {
                MountType::BIND
            } else {
                MountType::VOLUME
            }),
            read_only: Some(item.read_only),
            bind_options: item.bind.then_some(MountBindOptions {
                create_mountpoint: Some(true),
                ..MountBindOptions::default()
            }),
            ..Mount::default()
        })
        .collect()
}

fn exec_command(value: &str) -> Result<Vec<String>, DockerError> {
    let command = if value.trim().is_empty() {
        "/bin/sh"
    } else {
        value
    };
    let args = split_command(command)?;
    if args.is_empty() {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(args)
}

fn network_row(item: Network) -> NetworkRow {
    let configs = item.ipam.as_ref().and_then(|ipam| ipam.config.as_ref());
    let join = |pick: fn(&bollard::models::IpamConfig) -> &Option<String>| {
        configs
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| pick(row).clone())
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    NetworkRow {
        id: item.id.unwrap_or_default(),
        name: item.name.unwrap_or_default(),
        driver: item.driver.unwrap_or_default(),
        scope: item.scope.unwrap_or_default(),
        subnet: join(|row| &row.subnet),
        gateway: join(|row| &row.gateway),
        ipv6: item.enable_ipv6.unwrap_or(false),
    }
}

fn port_config(
    ports: &[super::model::PublishedPort],
) -> Result<
    (
        Option<Vec<String>>,
        Option<HashMap<String, Option<Vec<PortBinding>>>>,
    ),
    DockerError,
> {
    if ports.is_empty() {
        return Ok((None, None));
    }
    let mut exposed = Vec::new();
    let mut host_ports = Vec::new();
    let mut bindings = HashMap::new();
    for port in ports {
        let protocol = if port.udp { "udp" } else { "tcp" };
        let key = format!("{}/{}", port.container, protocol);
        let host_key = format!("{}:{protocol}", port.host);
        if exposed.contains(&key) || host_ports.contains(&host_key) {
            return Err(DockerError::new("invalid_input"));
        }
        exposed.push(key.clone());
        host_ports.push(host_key);
        bindings.insert(
            key,
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(port.host.to_string()),
            }]),
        );
    }
    Ok((Some(exposed), Some(bindings)))
}

fn map_create_error(error: bollard::errors::Error) -> DockerError {
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("already in use") {
        return DockerError::new("name_taken");
    }
    map_bollard(error, false)
}

fn map_run_error(error: bollard::errors::Error) -> DockerError {
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("port is already allocated") || text.contains("address already in use") {
        return DockerError::new("port_in_use");
    }
    map_bollard(error, false)
}

fn usage_of(sample: &ContainerStatsResponse) -> ContainerUsage {
    ContainerUsage {
        cpu_percent: cpu_percent(sample),
        memory_bytes: memory_used(sample),
        memory_limit: memory_limit(sample),
    }
}

fn cpu_percent(sample: &ContainerStatsResponse) -> Option<f64> {
    let cpu = sample.cpu_stats.as_ref()?;
    let previous = sample.precpu_stats.as_ref()?;
    let total = cpu.cpu_usage.as_ref()?.total_usage?;
    let previous_total = previous.cpu_usage.as_ref()?.total_usage?;
    let system = cpu.system_cpu_usage?;
    let previous_system = previous.system_cpu_usage?;
    let cpu_delta = total.saturating_sub(previous_total) as f64;
    let system_delta = system.saturating_sub(previous_system) as f64;
    if system_delta <= 0.0 {
        return None;
    }
    let online = cpu
        .online_cpus
        .unwrap_or_else(|| {
            cpu.cpu_usage
                .as_ref()
                .and_then(|usage| usage.percpu_usage.as_ref().map(|cores| cores.len() as u32))
                .unwrap_or(1)
        })
        .max(1) as f64;
    let percent = (cpu_delta / system_delta) * online * 100.0;
    percent
        .is_finite()
        .then(|| (percent.max(0.0) * 10.0).round() / 10.0)
}

fn memory_used(sample: &ContainerStatsResponse) -> u64 {
    let Some(memory) = sample.memory_stats.as_ref() else {
        return 0;
    };
    if let Some(usage) = memory.usage {
        let cache = memory
            .stats
            .as_ref()
            .and_then(|stats| {
                stats
                    .get("inactive_file")
                    .copied()
                    .or_else(|| stats.get("cache").copied())
            })
            .unwrap_or(0);
        return usage.saturating_sub(cache.min(usage));
    }
    memory.privateworkingset.unwrap_or(0)
}

fn memory_limit(sample: &ContainerStatsResponse) -> Option<u64> {
    let memory = sample.memory_stats.as_ref()?;
    let limit = memory.limit?;
    (limit > 0 && limit < 1 << 60).then_some(limit)
}

struct RegistryCheck {
    tag: String,
    digests: Vec<String>,
    credentials: Option<bollard::auth::DockerCredentials>,
}

async fn registry_digest_state(docker: &Docker, job: &RegistryCheck) -> &'static str {
    match tokio::time::timeout(
        Duration::from_secs(10),
        docker.inspect_registry_image(&job.tag, job.credentials.clone()),
    )
    .await
    {
        Ok(Ok(info)) => super::updates::registry_state(
            &job.tag,
            &job.digests,
            info.descriptor.digest.as_deref(),
        ),
        _ => super::updates::UNKNOWN,
    }
}

async fn remove_replaced_image(docker: &Docker, reference: &str, previous_id: &str) {
    let current_id = match docker.inspect_image(reference).await {
        Ok(image) => image.id,
        Err(_) => return,
    };
    let old = match docker.inspect_image(previous_id).await {
        Ok(image) => image,
        Err(_) => return,
    };
    let tags = old.repo_tags.unwrap_or_default();
    let options = ListContainersOptionsBuilder::new().all(true).build();
    let Ok(containers) = docker.list_containers(Some(options)).await else {
        return;
    };
    let uses = containers
        .into_iter()
        .map(|item| super::updates::ImageUse {
            name: item
                .names
                .as_ref()
                .and_then(|names| names.first())
                .map(|name| name.trim_start_matches('/').to_string())
                .unwrap_or_default(),
            image: item.image.unwrap_or_default(),
            image_id: item.image_id.unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let Some(id) =
        super::updates::replaced_image_to_remove(previous_id, current_id.as_deref(), &tags, &uses)
    else {
        return;
    };
    let _ = docker
        .remove_image(&id, Some(RemoveImageOptionsBuilder::new().build()), None)
        .await;
}

fn image_name(value: &str) -> Result<&str, DockerError> {
    let value = value.trim();
    if valid_image_ref(value) {
        Ok(value)
    } else {
        Err(DockerError::new("invalid_input"))
    }
}

fn signature_saved(endpoint: &Endpoint) -> String {
    signature(
        endpoint.kind,
        endpoint.socket_path.as_deref(),
        endpoint.ssh_host_id,
        endpoint.remote_socket.as_deref(),
    )
}

fn signature_new(endpoint: &NormalizedEndpoint) -> String {
    signature(
        endpoint.kind,
        endpoint.socket_path.as_deref(),
        endpoint.ssh_host_id,
        endpoint.remote_socket.as_deref(),
    )
}

fn signature(
    kind: EndpointKind,
    socket: Option<&str>,
    ssh_host: Option<i64>,
    remote: Option<&str>,
) -> String {
    match kind {
        EndpointKind::Local => format!("local:{}", socket.unwrap_or("")),
        EndpointKind::Ssh => format!("ssh:{}:{}", ssh_host.unwrap_or(0), remote.unwrap_or("")),
    }
}

fn map_bollard(error: bollard::errors::Error, in_use: bool) -> DockerError {
    use bollard::errors::Error;
    let mapped = match error {
        Error::DockerResponseServerError {
            status_code,
            message,
        } => map_status(status_code, &message, in_use),
        Error::DockerStreamError { error } => map_status(0, &error, in_use),
        Error::RequestTimeoutError => DockerError::new("timeout"),
        Error::SocketNotFoundError(_) => DockerError::new("unreachable"),
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
    if lower.contains("permission denied")
        || lower.contains("access is denied")
        || lower.contains("operation not permitted")
    {
        DockerError::new("permission_denied")
    } else if lower.contains("timed out") || lower.contains("timeout") {
        DockerError::new("timeout")
    } else {
        DockerError::new("unreachable")
    }
}

async fn run_exec<S, W>(
    docker: Docker,
    exec_id: String,
    size: (u16, u16),
    mut stream: S,
    mut input: W,
    output: Channel<InvokeResponseBody>,
    events: Channel<Value>,
    mut input_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut resize_rx: mpsc::UnboundedReceiver<(u16, u16)>,
    mut cancel: watch::Receiver<bool>,
) where
    S: futures_util::Stream<Item = Result<bollard::container::LogOutput, bollard::errors::Error>>
        + Unpin,
    W: AsyncWrite + Unpin,
{
    let _ = docker
        .resize_exec(
            &exec_id,
            bollard::exec::ResizeExecOptions {
                width: size.0,
                height: size.1,
            },
        )
        .await;
    emit(&events, json!({"type": "state", "state": "ready"}));
    loop {
        tokio::select! {
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    break;
                }
            }
            size = resize_rx.recv() => {
                let Some((width, height)) = size else { break };
                let _ = docker
                    .resize_exec(&exec_id, bollard::exec::ResizeExecOptions { width, height })
                    .await;
            }
            data = input_rx.recv() => {
                let Some(data) = data else { break };
                if input.write_all(&data).await.is_err() || input.flush().await.is_err() {
                    break;
                }
            }
            item = stream.next() => {
                match item {
                    Some(Ok(chunk)) => {
                        let bytes = chunk.as_ref();
                        if !bytes.is_empty() && output.send(InvokeResponseBody::Raw(bytes.to_vec())).is_err() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        }
    }
    emit(
        &events,
        json!({"type": "closed", "code": "disconnected", "params": {}}),
    );
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

fn project_text(draft: &compose::ComposeProjectRequest) -> Result<String, DockerError> {
    match draft.source.as_str() {
        "file" => {
            compose_path(&draft.file_path)?;
            let path = Path::new(draft.file_path.trim());
            let _ = compose::load_compose_file(path)?;
            let text =
                std::fs::read_to_string(path).map_err(|_| DockerError::new("invalid_input"))?;
            if text.len() > 1024 * 1024 {
                return Err(DockerError::new("invalid_input"));
            }
            Ok(text)
        }
        "yaml" => {
            if draft.yaml.len() > 1024 * 1024 || draft.yaml.trim().is_empty() {
                return Err(DockerError::new("invalid_input"));
            }
            let _ = compose::parse_compose(&draft.yaml, Path::new("docker-compose.yml"))?;
            Ok(draft.yaml.clone())
        }
        _ => Err(DockerError::new("invalid_input")),
    }
}

fn host_directory_for(endpoint: &Endpoint, value: &str) -> Result<String, DockerError> {
    match endpoint.kind {
        EndpointKind::Local => {
            #[cfg(windows)]
            {
                compose::host_directory(value, compose::HostPathStyle::Windows)
            }
            #[cfg(not(windows))]
            {
                compose::host_directory(value, compose::HostPathStyle::Unix)
            }
        }
        EndpointKind::Ssh => compose::host_directory(value, compose::HostPathStyle::Unix),
    }
}

async fn write_remote_compose(
    session: &AsyncMutex<Option<client::Handle<ClientHandler>>>,
    directory: &str,
    text: &str,
) -> Result<String, DockerError> {
    let sftp = open_compose_sftp(session).await?;
    let result = write_remote_compose_file(&sftp, directory, text).await;
    let _ = sftp.close().await;
    result
}

async fn write_remote_compose_file(
    sftp: &SftpSession,
    directory: &str,
    text: &str,
) -> Result<String, DockerError> {
    ensure_remote_dir(sftp, directory).await?;
    let destination = format!("{directory}/docker-compose.yml");
    match remote_compose_kind(sftp, &destination).await? {
        RemoteComposeKind::Missing => {}
        RemoteComposeKind::File | RemoteComposeKind::Dir | RemoteComposeKind::Other => {
            return Err(DockerError::new("compose_exists"));
        }
    }
    let temporary = format!("{directory}/.docker-compose.yml.pixo.tmp");
    if let Err(error) = write_remote_text(sftp, &temporary, text).await {
        let _ = sftp.remove_file(&temporary).await;
        return Err(error);
    }
    if sftp.rename(&temporary, &destination).await.is_err() {
        let _ = sftp.remove_file(&temporary).await;
        let exists = matches!(
            remote_compose_kind(sftp, &destination).await,
            Ok(RemoteComposeKind::File | RemoteComposeKind::Dir | RemoteComposeKind::Other)
        );
        return Err(DockerError::new(if exists {
            "compose_exists"
        } else {
            "unreachable"
        }));
    }
    Ok(destination)
}

async fn ensure_remote_dir(sftp: &SftpSession, directory: &str) -> Result<(), DockerError> {
    let mut current = String::new();
    for part in directory.split('/').filter(|part| !part.is_empty()) {
        current.push('/');
        current.push_str(part);
        match remote_compose_kind(sftp, &current).await? {
            RemoteComposeKind::Dir => {}
            RemoteComposeKind::Missing => {
                sftp.create_dir(&current)
                    .await
                    .map_err(|error| map_sftp_docker(&error))?;
            }
            RemoteComposeKind::File | RemoteComposeKind::Other => {
                return Err(DockerError::new("invalid_input"));
            }
        }
    }
    Ok(())
}

async fn write_remote_text(sftp: &SftpSession, path: &str, text: &str) -> Result<(), DockerError> {
    let mut remote = sftp
        .open_with_flags(
            path,
            OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE,
        )
        .await
        .map_err(|error| map_sftp_docker(&error))?;
    if let Err(error) = remote.write_all(text.as_bytes()).await {
        let _ = remote.close().await;
        return Err(map_remote_io(&error));
    }
    remote.close().await.map_err(|error| map_remote_io(&error))
}

async fn list_remote_host_dirs(
    session: &AsyncMutex<Option<client::Handle<ClientHandler>>>,
    raw: &str,
) -> Result<HostDirListing, DockerError> {
    let sftp = open_compose_sftp(session).await?;
    let result = read_remote_dirs(&sftp, raw).await;
    let _ = sftp.close().await;
    result
}

async fn read_remote_dirs(sftp: &SftpSession, raw: &str) -> Result<HostDirListing, DockerError> {
    let path = remote_browse_start(raw)?;
    let listed = sftp.read_dir(&path).await.map_err(map_browse_sftp)?;
    let mut entries = Vec::new();
    let mut seen = 0;
    for entry in listed {
        seen += 1;
        if seen > 4000 {
            break;
        }
        let name = entry.file_name();
        if !crate::ssh::remote::valid_component(&name) {
            continue;
        }
        let child = remote_child(&path, &name, &entry.path());
        let is_dir = match entry.file_type() {
            FileType::Dir => true,
            FileType::Symlink => match sftp.metadata(&child).await {
                Ok(meta) => matches!(meta.file_type(), FileType::Dir),
                Err(_) => false,
            },
            _ => false,
        };
        if is_dir {
            entries.push(HostDirEntry { name, path: child });
        }
    }
    entries.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    entries.truncate(500);
    let parent = crate::ssh::remote::parent_remote(&path);
    Ok(HostDirListing {
        path,
        parent,
        entries,
    })
}

fn remote_browse_start(raw: &str) -> Result<String, DockerError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok("/".to_string());
    }
    crate::ssh::remote::normalize_absolute(raw).ok_or_else(|| DockerError::new("invalid_input"))
}

fn remote_child(dir: &str, name: &str, given: &str) -> String {
    if given.starts_with('/') && !given.contains('\0') {
        return given.to_string();
    }
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

fn map_browse_sftp(error: russh_sftp::client::error::Error) -> DockerError {
    use russh_sftp::client::error::Error;
    match error {
        Error::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => DockerError::new("dir_missing"),
            StatusCode::PermissionDenied => DockerError::new("dir_denied"),
            StatusCode::NoConnection | StatusCode::ConnectionLost => {
                DockerError::new("disconnected")
            }
            _ => DockerError::new("unreachable"),
        },
        Error::Timeout => DockerError::new("timeout"),
        _ => DockerError::new("unreachable"),
    }
}

async fn open_compose_sftp(
    session: &AsyncMutex<Option<client::Handle<ClientHandler>>>,
) -> Result<SftpSession, DockerError> {
    let channel = {
        let guard = session.lock().await;
        let handle = guard
            .as_ref()
            .ok_or_else(|| DockerError::new("disconnected"))?;
        match handle.channel_open_session().await {
            Ok(channel) => channel,
            Err(_) if handle.is_closed() => return Err(DockerError::new("disconnected")),
            Err(_) => return Err(DockerError::new("sftp_unavailable")),
        }
    };
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|_| DockerError::new("sftp_unavailable"))?;
    let config = russh_sftp::client::Config {
        max_packet_len: 256 * 1024,
        max_concurrent_reads: 4,
        max_concurrent_writes: 4,
        max_write_packet_len: 256 * 1024,
        request_timeout_secs: 60,
    };
    SftpSession::new_with_config(channel.into_stream(), config)
        .await
        .map_err(|_| DockerError::new("sftp_unavailable"))
}

async fn start_remote_compose(
    session: &AsyncMutex<Option<client::Handle<ClientHandler>>>,
    project: &str,
    directory: &str,
    file: &str,
) -> Result<(), DockerError> {
    let location = compose::ComposeLocation {
        directory: directory.to_string(),
        file: file.to_string(),
    };
    let command = compose::compose_remote_command(compose::ComposeAction::Up, project, &location)?;
    run_remote_compose(session, command, "compose_start_failed").await
}

async fn run_remote_compose(
    session: &AsyncMutex<Option<client::Handle<ClientHandler>>>,
    command: String,
    failure: &'static str,
) -> Result<(), DockerError> {
    let mut channel = open_remote_command(session, command).await?;
    let status = wait_remote_status(&mut channel).await;
    finish_remote_status(status, failure)
}

async fn remote_shell(
    session: &AsyncMutex<Option<client::Handle<ClientHandler>>>,
    script: &str,
) -> Result<super::mirrors::ShellOutput, DockerError> {
    let mut channel = open_remote_command(session, script.to_string()).await?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut status = None;
    let mut truncated = false;
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { data }) => {
                if stdout.len() + data.len() > 256 * 1024 {
                    truncated = true;
                } else {
                    stdout.extend_from_slice(&data);
                }
            }
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                let room = 8 * 1024usize.saturating_sub(stderr.len());
                if room > 0 {
                    let take = room.min(data.len());
                    stderr.extend_from_slice(&data[..take]);
                }
            }
            Some(message) => {
                if remote_message_ends(&message, &mut status) {
                    break;
                }
            }
            None => break,
        }
    }
    if truncated {
        stdout.clear();
    }
    Ok(super::mirrors::ShellOutput {
        status: status.map(|code| code as i32),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        truncated,
    })
}

async fn local_shell(script: String) -> Result<super::mirrors::ShellOutput, DockerError> {
    tokio::task::spawn_blocking(move || run_local_shell(&script))
        .await
        .map_err(|_| DockerError::new("mirror_apply"))?
}

fn run_local_shell(script: &str) -> Result<super::mirrors::ShellOutput, DockerError> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|_| {
            DockerError::new(if cfg!(windows) {
                "mirror_unsupported"
            } else {
                "mirror_apply"
            })
        })?;
    Ok(super::mirrors::capture(
        output.status.code(),
        &output.stdout,
        &output.stderr,
    ))
}

fn podman_engine(version: &bollard::models::SystemVersion) -> bool {
    let component = version.components.as_ref().is_some_and(|items| {
        items
            .iter()
            .any(|item| item.name.to_ascii_lowercase().contains("podman"))
    });
    let platform = version
        .platform
        .as_ref()
        .is_some_and(|item| item.name.to_ascii_lowercase().contains("podman"));
    component || platform
}

fn configured_mirrors(info: &bollard::models::SystemInfo) -> Vec<String> {
    let Some(config) = info.registry_config.as_ref() else {
        return Vec::new();
    };
    if let Some(mirrors) = config
        .mirrors
        .as_ref()
        .filter(|items| items.iter().any(|item| !item.trim().is_empty()))
    {
        return mirrors.clone();
    }
    config
        .index_configs
        .as_ref()
        .and_then(|indexes| indexes.get("docker.io"))
        .and_then(|index| index.mirrors.clone())
        .unwrap_or_default()
}

async fn open_remote_command(
    session: &AsyncMutex<Option<client::Handle<ClientHandler>>>,
    command: String,
) -> Result<russh::Channel<client::Msg>, DockerError> {
    let channel = {
        let guard = session.lock().await;
        let handle = guard
            .as_ref()
            .ok_or_else(|| DockerError::new("disconnected"))?;
        match handle.channel_open_session().await {
            Ok(channel) => channel,
            Err(_) if handle.is_closed() => return Err(DockerError::new("disconnected")),
            Err(_) => return Err(DockerError::new("unreachable")),
        }
    };
    if channel.exec(true, command.into_bytes()).await.is_err() {
        return Err(DockerError::new("unreachable"));
    }
    Ok(channel)
}

async fn wait_remote_status(channel: &mut russh::Channel<client::Msg>) -> Option<u32> {
    let mut status = None;
    loop {
        match channel.wait().await {
            Some(message) => {
                if remote_message_ends(&message, &mut status) {
                    break;
                }
            }
            None => break,
        }
    }
    status
}

fn remote_message_ends(message: &ChannelMsg, status: &mut Option<u32>) -> bool {
    match message {
        ChannelMsg::ExitStatus { exit_status } => {
            *status = Some(*exit_status);
            false
        }
        // The server often closes stdout before it sends the exit status.
        // Stopping at Eof reports a successful `compose up` as a failure.
        ChannelMsg::Close => true,
        _ => false,
    }
}

fn finish_remote_status(status: Option<u32>, failure: &'static str) -> Result<(), DockerError> {
    match status {
        Some(0) => Ok(()),
        Some(127) => Err(DockerError::new("compose_cli")),
        _ => Err(DockerError::new(failure)),
    }
}

enum RemoteComposeKind {
    Missing,
    File,
    Dir,
    Other,
}

async fn remote_compose_kind(
    sftp: &SftpSession,
    path: &str,
) -> Result<RemoteComposeKind, DockerError> {
    match sftp.metadata(path).await {
        Ok(meta) => Ok(match meta.file_type() {
            FileType::Dir => RemoteComposeKind::Dir,
            FileType::File => RemoteComposeKind::File,
            _ => RemoteComposeKind::Other,
        }),
        Err(error) if sftp_missing(&error) => Ok(RemoteComposeKind::Missing),
        Err(error) => Err(map_sftp_docker(&error)),
    }
}

fn sftp_missing(error: &russh_sftp::client::error::Error) -> bool {
    matches!(
        error,
        russh_sftp::client::error::Error::Status(status) if status.status_code == StatusCode::NoSuchFile
    )
}

fn map_sftp_docker(error: &russh_sftp::client::error::Error) -> DockerError {
    use russh_sftp::client::error::Error;
    match error {
        Error::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => DockerError::new("compose_dir"),
            StatusCode::PermissionDenied => DockerError::new("permission_denied"),
            StatusCode::NoConnection | StatusCode::ConnectionLost => {
                DockerError::new("disconnected")
            }
            _ => DockerError::new("unreachable"),
        },
        Error::Timeout => DockerError::new("timeout"),
        _ => DockerError::new("unreachable"),
    }
}

fn map_remote_io(error: &std::io::Error) -> DockerError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        DockerError::new("permission_denied")
    } else if error.kind() == std::io::ErrorKind::TimedOut {
        DockerError::new("timeout")
    } else {
        DockerError::new("unreachable")
    }
}

fn start_compose(endpoint: &Endpoint, project: &str, file: &Path) -> Result<(), DockerError> {
    let directory = file
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_string_lossy()
        .into_owned();
    let location = compose::ComposeLocation {
        directory,
        file: file.to_string_lossy().into_owned(),
    };
    run_local_compose(
        endpoint,
        &location,
        project,
        compose::ComposeAction::Up,
        "compose_start_failed",
    )
}

fn run_local_compose(
    endpoint: &Endpoint,
    location: &compose::ComposeLocation,
    project: &str,
    action: compose::ComposeAction,
    failure: &'static str,
) -> Result<(), DockerError> {
    let mut command = local_compose_command(endpoint, location, project, action)?;
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    let output = command
        .output()
        .map_err(|_| DockerError::new("compose_cli"))?;
    if output.status.success() {
        Ok(())
    } else {
        eprintln!("docker compose {failure}: exit {}", output.status);
        if output.status.code() == Some(127) {
            Err(DockerError::new("compose_cli"))
        } else {
            Err(DockerError::new(failure))
        }
    }
}

fn local_compose_command(
    endpoint: &Endpoint,
    location: &compose::ComposeLocation,
    project: &str,
    action: compose::ComposeAction,
) -> Result<std::process::Command, DockerError> {
    let kind = endpoint.kind.as_str();
    let address = match endpoint.kind {
        EndpointKind::Local => resolve_local(endpoint.socket_path.as_deref())?,
        EndpointKind::Ssh => return Err(DockerError::new("compose_path_unsupported")),
    };
    let docker_host = compose::docker_host_value(kind, &address)?;
    let mut command = std::process::Command::new("docker");
    command
        .arg("compose")
        .arg("--project-name")
        .arg(project)
        .arg("--file")
        .arg(&location.file)
        .args(match action {
            compose::ComposeAction::Build => &["build"][..],
            compose::ComposeAction::Up => &["up", "--detach"][..],
            compose::ComposeAction::Down => &["down"][..],
            compose::ComposeAction::Logs => {
                &["logs", "--tail", "200", "--follow", "--no-color"][..]
            }
        })
        .current_dir(&location.directory)
        .env("DOCKER_HOST", docker_host)
        .env_remove("DOCKER_TLS_VERIFY")
        .env_remove("DOCKER_CERT_PATH")
        .stdin(std::process::Stdio::null());
    Ok(command)
}

struct KillChild(Option<std::process::Child>);

impl Drop for KillChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

async fn stream_local_compose_logs(
    endpoint: Endpoint,
    project: String,
    location: compose::ComposeLocation,
    events: Channel<Value>,
    mut cancel: watch::Receiver<bool>,
) {
    let mut command =
        match local_compose_command(&endpoint, &location, &project, compose::ComposeAction::Logs) {
            Ok(command) => command,
            Err(error) => {
                emit(
                    &events,
                    json!({"type": "closed", "code": error.code, "params": error.params}),
                );
                return;
            }
        };
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => KillChild(Some(child)),
        Err(_) => {
            emit(
                &events,
                json!({"type": "closed", "code": "compose_cli", "params": {}}),
            );
            return;
        }
    };
    let stdout = child.0.as_mut().and_then(|item| item.stdout.take());
    let stderr = child.0.as_mut().and_then(|item| item.stderr.take());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    if let Some(stdout) = stdout {
        let tx = tx.clone();
        std::thread::spawn(move || pump_compose_output(stdout, "stdout", tx));
    }
    if let Some(stderr) = stderr {
        std::thread::spawn(move || pump_compose_output(stderr, "stderr", tx));
    } else {
        drop(tx);
    }
    let mut stopped = false;
    loop {
        tokio::select! {
            _ = cancel.wait_for(|stopped| *stopped) => {
                stopped = true;
                break;
            }
            item = rx.recv() => {
                match item {
                    Some(value) => emit(&events, value),
                    None => break,
                }
            }
        }
    }
    if stopped {
        emit(
            &events,
            json!({"type": "closed", "code": "ok", "params": {}}),
        );
        return;
    }
    let status = child.0.as_mut().and_then(|item| item.wait().ok());
    child.0 = None;
    match status.and_then(|item| item.code()) {
        Some(0) | None => emit(
            &events,
            json!({"type": "closed", "code": "ok", "params": {}}),
        ),
        Some(127) => emit(
            &events,
            json!({"type": "closed", "code": "compose_cli", "params": {}}),
        ),
        Some(_) => emit(
            &events,
            json!({"type": "closed", "code": "compose_logs_failed", "params": {}}),
        ),
    }
}

fn pump_compose_output(
    mut reader: impl Read,
    stream: &str,
    tx: tokio::sync::mpsc::UnboundedSender<Value>,
) {
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(size) => {
                let text = String::from_utf8_lossy(&buffer[..size]).into_owned();
                if tx
                    .send(json!({"type": "data", "stream": stream, "text": text}))
                    .is_err()
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

async fn stream_remote_compose_logs(
    session: Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>,
    command: String,
    events: Channel<Value>,
    mut cancel: watch::Receiver<bool>,
) {
    let mut channel = match open_remote_command(&session, command).await {
        Ok(channel) => channel,
        Err(error) => {
            emit(
                &events,
                json!({"type": "closed", "code": error.code, "params": error.params}),
            );
            return;
        }
    };
    let mut status = None;
    let mut stopped = false;
    loop {
        tokio::select! {
            _ = cancel.wait_for(|stopped| *stopped) => {
                stopped = true;
                break;
            }
            message = channel.wait() => {
                match message {
                    Some(ChannelMsg::Data { data }) => emit_compose_text(&events, "stdout", &data),
                    Some(ChannelMsg::ExtendedData { data, ext: 1 }) => emit_compose_text(&events, "stderr", &data),
                    Some(ChannelMsg::ExitStatus { exit_status }) => status = Some(exit_status),
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                    Some(_) => {}
                }
            }
        }
    }
    let _ = channel.close().await;
    if stopped {
        emit(
            &events,
            json!({"type": "closed", "code": "ok", "params": {}}),
        );
        return;
    }
    match status {
        Some(0) | None => emit(
            &events,
            json!({"type": "closed", "code": "ok", "params": {}}),
        ),
        Some(127) => emit(
            &events,
            json!({"type": "closed", "code": "compose_cli", "params": {}}),
        ),
        Some(_) => emit(
            &events,
            json!({"type": "closed", "code": "compose_logs_failed", "params": {}}),
        ),
    }
}

fn emit_compose_text(events: &Channel<Value>, stream: &str, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    let text = String::from_utf8_lossy(data).into_owned();
    emit(
        events,
        json!({"type": "data", "stream": stream, "text": text}),
    );
}

fn read_local_compose(path: &str) -> Result<String, DockerError> {
    let meta = std::fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            DockerError::new("compose_file_missing")
        } else if error.kind() == std::io::ErrorKind::PermissionDenied {
            DockerError::new("permission_denied")
        } else {
            DockerError::new("unreachable")
        }
    })?;
    if !meta.is_file() || meta.len() > 1024 * 1024 {
        return Err(DockerError::new("compose_file_missing"));
    }
    std::fs::read_to_string(path).map_err(|_| DockerError::new("compose_file_missing"))
}

async fn read_remote_compose(
    session: &AsyncMutex<Option<client::Handle<ClientHandler>>>,
    path: &str,
) -> Result<String, DockerError> {
    let sftp = open_compose_sftp(session).await?;
    let result = read_remote_compose_file(&sftp, path).await;
    let _ = sftp.close().await;
    result
}

async fn read_remote_compose_file(sftp: &SftpSession, path: &str) -> Result<String, DockerError> {
    let meta = sftp.metadata(path).await.map_err(|error| {
        if sftp_missing(&error) {
            DockerError::new("compose_file_missing")
        } else {
            map_sftp_docker(&error)
        }
    })?;
    if meta.file_type() != FileType::File || meta.size.unwrap_or(0) > 1024 * 1024 {
        return Err(DockerError::new("compose_file_missing"));
    }
    let bytes = sftp.read(path).await.map_err(|error| {
        if sftp_missing(&error) {
            DockerError::new("compose_file_missing")
        } else {
            map_sftp_docker(&error)
        }
    })?;
    if bytes.len() > 1024 * 1024 {
        return Err(DockerError::new("compose_file_missing"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn path_style(endpoint: &Endpoint) -> compose::HostPathStyle {
    match endpoint.kind {
        EndpointKind::Local => {
            #[cfg(windows)]
            {
                compose::HostPathStyle::Windows
            }
            #[cfg(not(windows))]
            {
                compose::HostPathStyle::Unix
            }
        }
        EndpointKind::Ssh => compose::HostPathStyle::Unix,
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

#[cfg(test)]
mod tests {
    use super::remote_message_ends;
    use russh::ChannelMsg;

    #[test]
    fn compose_exit_status_arrives_after_stdout_eof() {
        let mut status = None;
        assert!(!remote_message_ends(&ChannelMsg::Eof, &mut status));
        assert!(status.is_none());
        assert!(!remote_message_ends(
            &ChannelMsg::ExitStatus { exit_status: 0 },
            &mut status
        ));
        assert_eq!(status, Some(0));
        assert!(remote_message_ends(&ChannelMsg::Close, &mut status));
    }
}
