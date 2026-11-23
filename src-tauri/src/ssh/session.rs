use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::client::{self, Msg};
use russh::{ChannelMsg, Disconnect, Sig};
use serde_json::{Value, json};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::{mpsc, watch};

use super::cwd::CwdState;
use super::dial::{self, Bridge, ClientHandler};
use super::error::{SshError, log_ssh};
use super::flow::wait_for_output_room;
use super::forward::parse_forward;
use super::grant;
use super::known_hosts::{KnownStore, user_known_hosts_path};
use super::model::{
    AuthKind, ForwardView, Host, HostDraft, Snippet, SnippetDraft, SshStatus, normalize,
};
use super::remote::normalize_absolute;
use super::secret;
use super::shell::{cwd_hook, shell_single_quote};
use super::store::Store;
use super::transfer::{self, DecisionGate, TransferSlot};

const OUTPUT_FLUSH: Duration = Duration::from_millis(16);
const OUTPUT_LIMIT: usize = 64 * 1024;
const REPLAY_TTL: Duration = Duration::from_secs(30 * 60);
const REPLAY_LIMIT: usize = 32;

#[derive(Clone)]
enum TransferPlan {
    Upload { paths: Vec<PathBuf> },
    Download { root: PathBuf, remote: Vec<String> },
}

struct Replay {
    plan: TransferPlan,
    at: Instant,
}

enum Input {
    Write(Vec<u8>),
    Interrupt,
    Resize { cols: u32, rows: u32 },
    Close,
}

struct ForwardRun {
    local_port: u16,
    remote_host: String,
    remote_port: u16,
    cancel: watch::Sender<bool>,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl Drop for ForwardRun {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        self.task.abort();
    }
}

#[derive(Clone)]
struct SessionControl {
    bridge: Arc<Bridge>,
    input: mpsc::UnboundedSender<Input>,
    handle: Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>,
    cwd: Arc<Mutex<CwdState>>,
    transfers: Arc<Mutex<HashMap<u64, TransferSlot>>>,
    forwards: Arc<Mutex<HashMap<u64, ForwardRun>>>,
}

struct Inner {
    app: AppHandle,
    store: Arc<Mutex<Store>>,
    known: Arc<Mutex<KnownStore>>,
    sessions: Mutex<HashMap<u64, SessionControl>>,
    replays: Mutex<HashMap<u64, Replay>>,
    next_id: AtomicU64,
    next_task: AtomicU64,
}

pub struct SshState {
    inner: Arc<Inner>,
}

struct SessionGuard {
    inner: Arc<Inner>,
    id: u64,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        lock(&self.inner.sessions).remove(&self.id);
    }
}

pub fn initialize(app: AppHandle) -> Result<SshState, SshError> {
    let root = app.path().app_data_dir().map_err(|error| {
        eprintln!("ssh path: {error}");
        SshError::new("storage")
    })?;
    let dir = root.join("ssh");
    std::fs::create_dir_all(&dir).map_err(storage)?;
    let database = dir.join("hosts.db");
    let store = Store::open(&database)?;
    tighten(&database);
    let known = KnownStore::new(dir.join("known_hosts"), user_known_hosts_path());
    Ok(SshState {
        inner: Arc::new(Inner {
            app,
            store: Arc::new(Mutex::new(store)),
            known: Arc::new(Mutex::new(known)),
            sessions: Mutex::new(HashMap::new()),
            replays: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            next_task: AtomicU64::new(1),
        }),
    })
}

impl SshState {
    pub fn status(&self) -> Result<SshStatus, SshError> {
        let store = lock(&self.inner.store);
        Ok(self.status_of(store.enabled()?, store.keepalive()?))
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<SshStatus, SshError> {
        lock(&self.inner.store).set_enabled(enabled)?;
        if !enabled {
            self.disconnect_all();
            lock(&self.inner.replays).clear();
        }
        let _ = self.inner.app.emit("ssh://enabled", enabled);
        self.status()
    }

    pub fn set_keepalive(&self, seconds: u32) -> Result<SshStatus, SshError> {
        lock(&self.inner.store).set_keepalive(seconds)?;
        self.status()
    }

    fn status_of(&self, enabled: bool, keepalive_secs: u32) -> SshStatus {
        SshStatus {
            enabled,
            keyring_available: secret::available(),
            agent_available: super::agent::available(),
            keepalive_secs,
        }
    }

    pub fn list(&self) -> Result<Vec<Host>, SshError> {
        lock(&self.inner.store).list()
    }

    pub fn tool_enabled(&self) -> Result<bool, SshError> {
        lock(&self.inner.store).enabled()
    }

    pub fn load_host(&self, id: i64) -> Result<Host, SshError> {
        lock(&self.inner.store).get(id)
    }

    pub fn dial_env(&self) -> dial::DialEnv {
        dial::DialEnv {
            store: Arc::clone(&self.inner.store),
            known: Arc::clone(&self.inner.known),
        }
    }

    pub fn jump_hosts(&self, host: &Host) -> Result<Vec<Host>, SshError> {
        let store = lock(&self.inner.store);
        host.jumps.iter().map(|id| store.get(*id)).collect()
    }

    pub fn snippets(&self) -> Result<Vec<Snippet>, SshError> {
        let store = lock(&self.inner.store);
        if !store.enabled()? {
            return Err(SshError::new("disabled"));
        }
        store.snippets()
    }

    pub fn save_snippet(&self, draft: &SnippetDraft) -> Result<Snippet, SshError> {
        let store = lock(&self.inner.store);
        if !store.enabled()? {
            return Err(SshError::new("disabled"));
        }
        store.save_snippet(draft)
    }

    pub fn delete_snippet(&self, id: i64) -> Result<(), SshError> {
        let store = lock(&self.inner.store);
        if !store.enabled()? {
            return Err(SshError::new("disabled"));
        }
        store.delete_snippet(id)
    }

    pub fn save(&self, draft: HostDraft) -> Result<Host, SshError> {
        let host = normalize(draft)?;
        if host.remember && !secret::available() {
            return Err(SshError::new("keyring_unavailable"));
        }
        let previous = match host.id {
            Some(id) => Some(lock(&self.inner.store).get(id)?),
            None => None,
        };
        if let Some(old) = &previous {
            let identity_changed = old.host != host.host
                || old.port != host.port
                || old.username != host.username
                || old.auth != host.auth
                || old.key_path != host.key_path;
            if old.remember && (identity_changed || !host.remember) {
                secret::delete_host(old.id)?;
            }
        }
        let saved = lock(&self.inner.store).save(&host, now_millis())?;
        self.notify_hosts();
        Ok(saved)
    }

    pub fn delete(&self, id: i64, detach: bool) -> Result<(), SshError> {
        let host = lock(&self.inner.store).get(id)?;
        let names = lock(&self.inner.store).dependent_names(id)?;
        if !names.is_empty() && !detach {
            return Err(SshError::with_params(
                "jump_in_use",
                serde_json::json!({ "hosts": names.join("、") }),
            ));
        }
        if host.remember {
            secret::delete_host(id)?;
        }
        lock(&self.inner.store).delete(id, true)?;
        self.notify_hosts();
        Ok(())
    }

    fn notify_hosts(&self) {
        let _ = self.inner.app.emit("ssh://hosts", ());
    }

    pub fn config_preview(&self) -> Result<Vec<super::config_import::ConfigPreview>, SshError> {
        let entries = super::config_import::load_user_config()?;
        let hosts = lock(&self.inner.store).list()?;
        let existing = hosts
            .iter()
            .map(|host| (host.host.clone(), host.port, host.username.clone()))
            .collect::<Vec<_>>();
        Ok(super::config_import::preview(&entries, &existing))
    }

    pub fn config_import(&self, indexes: &[u32]) -> Result<Vec<Host>, SshError> {
        if indexes.len() > 500 {
            return Err(SshError::new("invalid_input"));
        }
        let entries = super::config_import::load_user_config()?;
        let chosen = super::config_import::expand_selection(&entries, indexes);
        if chosen.is_empty() {
            return Err(SshError::new("invalid_input"));
        }
        let mut ids = HashMap::<String, i64>::new();
        let mut created = Vec::new();
        for index in chosen {
            let entry = &entries[index as usize];
            let host = self.save(HostDraft {
                id: None,
                name: entry.alias.clone(),
                group_name: None,
                host: entry.host.clone(),
                port: entry.port,
                username: entry.username.clone(),
                auth: if entry.key_path.is_some() {
                    AuthKind::Key
                } else {
                    AuthKind::Password
                },
                key_path: entry.key_path.clone(),
                remember: false,
                start_dir: None,
                term: None,
                jumps: Vec::new(),
                agent_forward: false,
            })?;
            ids.insert(entry.alias.to_ascii_lowercase(), host.id);
            created.push((host, entry.jumps.clone()));
        }
        let mut saved = Vec::new();
        for (host, jumps) in created {
            let jump_ids = jumps
                .iter()
                .filter_map(|name| ids.get(&name.to_ascii_lowercase()).copied())
                .collect::<Vec<_>>();
            if jump_ids.is_empty() || jump_ids.len() != jumps.len() {
                saved.push(host);
                continue;
            }
            saved.push(self.save(HostDraft {
                id: Some(host.id),
                name: host.name,
                group_name: host.group_name,
                host: host.host,
                port: host.port,
                username: host.username,
                auth: host.auth,
                key_path: host.key_path,
                remember: host.remember,
                start_dir: host.start_dir,
                term: Some(host.term),
                jumps: jump_ids,
                agent_forward: host.agent_forward,
            })?);
        }
        Ok(saved)
    }

    pub fn forget_key(&self, id: i64) -> Result<(), SshError> {
        let host = lock(&self.inner.store).get(id)?;
        lock(&self.inner.known).forget(&host.host, host.port)
    }

    pub fn connect(
        &self,
        host_id: i64,
        cols: u32,
        rows: u32,
        output: Channel<InvokeResponseBody>,
        events: Channel<Value>,
    ) -> Result<u64, SshError> {
        if !lock(&self.inner.store).enabled()? {
            return Err(SshError::new("disabled"));
        }
        let host = lock(&self.inner.store).get(host_id)?;
        let bridge = Bridge::new();
        let (input, input_rx) = mpsc::unbounded_channel();
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        lock(&self.inner.sessions).insert(
            id,
            SessionControl {
                bridge: Arc::clone(&bridge),
                input,
                handle: Arc::new(AsyncMutex::new(None)),
                cwd: Arc::new(Mutex::new(CwdState::new())),
                transfers: Arc::new(Mutex::new(HashMap::new())),
                forwards: Arc::new(Mutex::new(HashMap::new())),
            },
        );
        let inner = Arc::clone(&self.inner);
        tauri::async_runtime::spawn(async move {
            let _guard = SessionGuard {
                inner: Arc::clone(&inner),
                id,
            };
            run_session(
                inner,
                id,
                host,
                clamp_size(cols, rows),
                bridge,
                input_rx,
                output,
                events,
            )
            .await;
        });
        Ok(id)
    }

    pub async fn open_forward(
        &self,
        session_id: u64,
        local_port: u32,
        remote_host: &str,
        remote_port: u32,
    ) -> Result<ForwardView, SshError> {
        let target = parse_forward(local_port, remote_host, remote_port)?;
        let control = control(&self.inner, session_id)?;
        {
            let slot = control.handle.lock().await;
            if slot.as_ref().is_none_or(|handle| handle.is_closed()) {
                return Err(SshError::new("disconnected"));
            }
        }
        if lock(&control.forwards).len() >= 8 {
            return Err(SshError::new("forward_limit"));
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, target.local_port))
            .await
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AddrInUse {
                    SshError::new("port_in_use")
                } else {
                    eprintln!("ssh forward: {error}");
                    SshError::new("connect_failed")
                }
            })?;
        let (cancel, cancel_rx) = watch::channel(false);
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let handle = Arc::clone(&control.handle);
        let host = target.remote_host.clone();
        let port = u32::from(target.remote_port);
        let task = tauri::async_runtime::spawn(async move {
            accept_forward(listener, handle, host, port, cancel_rx).await;
        });
        lock(&control.forwards).insert(
            id,
            ForwardRun {
                local_port: target.local_port,
                remote_host: target.remote_host.clone(),
                remote_port: target.remote_port,
                cancel,
                task,
            },
        );
        Ok(ForwardView {
            id,
            session_id,
            local_port: target.local_port,
            remote_host: target.remote_host,
            remote_port: target.remote_port,
        })
    }

    pub fn list_forwards(&self, session_id: u64) -> Result<Vec<ForwardView>, SshError> {
        let control = control(&self.inner, session_id)?;
        let mut items = lock(&control.forwards)
            .iter()
            .map(|(id, run)| ForwardView {
                id: *id,
                session_id,
                local_port: run.local_port,
                remote_host: run.remote_host.clone(),
                remote_port: run.remote_port,
            })
            .collect::<Vec<_>>();
        items.sort_by_key(|item| item.local_port);
        Ok(items)
    }

    pub fn close_forward(&self, session_id: u64, id: u64) -> Result<(), SshError> {
        let control = control(&self.inner, session_id)?;
        lock(&control.forwards)
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| SshError::new("not_found"))
    }

    pub fn decide_host_key(&self, session_id: u64, accept: bool) -> Result<(), SshError> {
        let control = control(&self.inner, session_id)?;
        dial::decide_hostkey(&control.bridge, accept)
    }

    pub fn answer(
        &self,
        session_id: u64,
        prompt_id: u64,
        secret: Option<String>,
        remember: bool,
        cancel: bool,
    ) -> Result<(), SshError> {
        let control = control(&self.inner, session_id)?;
        dial::answer_prompt(&control.bridge, prompt_id, secret, remember, cancel)
    }

    pub fn write(&self, session_id: u64, data: Vec<u8>) -> Result<(), SshError> {
        if data.len() > 1024 * 1024 {
            return Err(SshError::new("invalid_input"));
        }
        let control = control(&self.inner, session_id)?;
        control
            .input
            .send(Input::Write(data))
            .map_err(|_| SshError::new("disconnected"))
    }

    pub fn interrupt(&self, session_id: u64) -> Result<(), SshError> {
        let control = control(&self.inner, session_id)?;
        control
            .input
            .send(Input::Interrupt)
            .map_err(|_| SshError::new("disconnected"))
    }

    pub fn ack(&self, session_id: u64, bytes: u64) -> Result<(), SshError> {
        if let Ok(control) = control(&self.inner, session_id) {
            control.bridge.output.release(bytes);
        }
        Ok(())
    }

    pub fn resize(&self, session_id: u64, cols: u32, rows: u32) -> Result<(), SshError> {
        let control = control(&self.inner, session_id)?;
        control
            .input
            .send(Input::Resize { cols, rows })
            .map_err(|_| SshError::new("disconnected"))
    }

    pub fn disconnect(&self, session_id: u64) -> Result<(), SshError> {
        let Some(control) = lock(&self.inner.sessions).get(&session_id).cloned() else {
            return Ok(());
        };
        cancel_session(&control);
        Ok(())
    }

    pub fn upload(
        &self,
        session_id: u64,
        token: String,
        progress: Channel<Value>,
    ) -> Result<u64, SshError> {
        let paths = grant::take(&token)?;
        self.launch_upload(session_id, paths, progress)
    }

    fn launch_upload(
        &self,
        session_id: u64,
        paths: Vec<PathBuf>,
        progress: Channel<Value>,
    ) -> Result<u64, SshError> {
        let control = control(&self.inner, session_id)?;
        let task_id = self.inner.next_task.fetch_add(1, Ordering::Relaxed);
        self.remember_plan(
            task_id,
            TransferPlan::Upload {
                paths: paths.clone(),
            },
        );
        let (cancel, cancel_rx) = watch::channel(false);
        let gate = Arc::new(DecisionGate::default());
        lock(&control.transfers).insert(
            task_id,
            TransferSlot {
                cancel,
                gate: Arc::clone(&gate),
            },
        );
        let handle = Arc::clone(&control.handle);
        let cwd = Arc::clone(&control.cwd);
        let transfers = Arc::clone(&control.transfers);
        let bridge = Arc::clone(&control.bridge);
        tauri::async_runtime::spawn(async move {
            let outcome = run_upload(
                handle,
                cwd,
                paths,
                task_id,
                progress.clone(),
                cancel_rx,
                gate,
            )
            .await;
            lock(&transfers).remove(&task_id);
            if let Err(error) = outcome {
                report_transfer_failure(&progress, task_id, error, *bridge.cancel.borrow());
            }
        });
        Ok(task_id)
    }

    pub async fn list_dir(
        &self,
        session_id: u64,
        path: String,
    ) -> Result<transfer::RemoteListing, SshError> {
        let control = control(&self.inner, session_id)?;
        let path = if path.trim().is_empty() {
            lock(&control.cwd).path().unwrap_or_else(|| "/".to_string())
        } else {
            normalize_absolute(&path).ok_or_else(|| SshError::new("invalid_input"))?
        };
        let sftp = open_sftp(&control.handle).await?;
        let listing = transfer::list_dir(&sftp, &path).await;
        let _ = sftp.close().await;
        listing
    }

    pub async fn make_dir(&self, session_id: u64, path: String) -> Result<(), SshError> {
        let control = control(&self.inner, session_id)?;
        let sftp = open_sftp(&control.handle).await?;
        let result = transfer::make_dir(&sftp, &path).await;
        let _ = sftp.close().await;
        result
    }

    pub async fn rename_path(
        &self,
        session_id: u64,
        from: String,
        to: String,
    ) -> Result<(), SshError> {
        let control = control(&self.inner, session_id)?;
        let sftp = open_sftp(&control.handle).await?;
        let result = transfer::rename_path(&sftp, &from, &to).await;
        let _ = sftp.close().await;
        result
    }

    pub async fn remove_paths(&self, session_id: u64, paths: Vec<String>) -> Result<(), SshError> {
        if paths.is_empty() || paths.len() > 100 {
            return Err(SshError::new("invalid_input"));
        }
        let control = control(&self.inner, session_id)?;
        let sftp = open_sftp(&control.handle).await?;
        let mut result = Ok(());
        for path in paths {
            if let Err(error) = transfer::remove_path(&sftp, &path).await {
                result = Err(error);
                break;
            }
        }
        let _ = sftp.close().await;
        result
    }

    pub fn download(
        &self,
        session_id: u64,
        token: String,
        remote_paths: Vec<String>,
        progress: Channel<Value>,
    ) -> Result<u64, SshError> {
        if remote_paths.is_empty() || remote_paths.len() > 500 {
            return Err(SshError::new("invalid_input"));
        }
        let mut paths = Vec::new();
        for path in remote_paths {
            paths.push(normalize_absolute(&path).ok_or_else(|| SshError::new("invalid_input"))?);
        }
        let local = grant::take(&token)?;
        if local.len() != 1 {
            return Err(SshError::new("invalid_input"));
        }
        let root = local.into_iter().next().unwrap_or_default();
        if !root.is_dir() {
            return Err(SshError::new("invalid_input"));
        }
        self.launch_download(session_id, root, paths, progress)
    }

    fn launch_download(
        &self,
        session_id: u64,
        root: PathBuf,
        paths: Vec<String>,
        progress: Channel<Value>,
    ) -> Result<u64, SshError> {
        if !root.is_dir() {
            return Err(SshError::new("invalid_input"));
        }
        let control = control(&self.inner, session_id)?;
        let task_id = self.inner.next_task.fetch_add(1, Ordering::Relaxed);
        self.remember_plan(
            task_id,
            TransferPlan::Download {
                root: root.clone(),
                remote: paths.clone(),
            },
        );
        let (cancel, cancel_rx) = watch::channel(false);
        let gate = Arc::new(DecisionGate::default());
        lock(&control.transfers).insert(
            task_id,
            TransferSlot {
                cancel,
                gate: Arc::clone(&gate),
            },
        );
        let handle = Arc::clone(&control.handle);
        let transfers = Arc::clone(&control.transfers);
        let bridge = Arc::clone(&control.bridge);
        tauri::async_runtime::spawn(async move {
            let outcome = run_download(
                handle,
                root,
                paths,
                task_id,
                progress.clone(),
                cancel_rx,
                gate,
            )
            .await;
            lock(&transfers).remove(&task_id);
            if let Err(error) = outcome {
                report_transfer_failure(&progress, task_id, error, *bridge.cancel.borrow());
            }
        });
        Ok(task_id)
    }

    pub fn retry_transfer(
        &self,
        session_id: u64,
        task_id: u64,
        progress: Channel<Value>,
    ) -> Result<u64, SshError> {
        let plan = lock(&self.inner.replays)
            .get(&task_id)
            .map(|replay| replay.plan.clone())
            .ok_or_else(|| SshError::new("not_found"))?;
        let new_id = match plan {
            TransferPlan::Upload { paths } => self.launch_upload(session_id, paths, progress)?,
            TransferPlan::Download { root, remote } => {
                self.launch_download(session_id, root, remote, progress)?
            }
        };
        self.forget_transfer(task_id);
        Ok(new_id)
    }

    pub fn forget_transfer(&self, task_id: u64) {
        lock(&self.inner.replays).remove(&task_id);
    }

    fn remember_plan(&self, task_id: u64, plan: TransferPlan) {
        let mut map = lock(&self.inner.replays);
        let now = Instant::now();
        map.retain(|_, replay| now.saturating_duration_since(replay.at) < REPLAY_TTL);
        if !map.contains_key(&task_id) && map.len() >= REPLAY_LIMIT {
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, replay)| replay.at)
                .map(|(id, _)| *id)
            {
                map.remove(&oldest);
            }
        }
        map.insert(task_id, Replay { plan, at: now });
    }

    pub fn decide_transfer(
        &self,
        task_id: u64,
        policy: &str,
        apply_all: bool,
    ) -> Result<(), SshError> {
        let policy = transfer::parse_policy(policy)?;
        let controls = lock(&self.inner.sessions)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for control in controls {
            if let Some(slot) = lock(&control.transfers).get(&task_id) {
                return slot.gate.answer(policy, apply_all);
            }
        }
        Err(SshError::new("not_found"))
    }

    pub fn cancel_transfer(&self, task_id: u64) -> Result<(), SshError> {
        let controls = lock(&self.inner.sessions)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for control in controls {
            if let Some(slot) = lock(&control.transfers).get(&task_id) {
                let _ = slot.cancel.send(true);
                return Ok(());
            }
        }
        Ok(())
    }

    fn disconnect_all(&self) {
        let controls = lock(&self.inner.sessions)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for control in controls {
            cancel_session(&control);
        }
    }
}

fn cancel_session(control: &SessionControl) {
    let _ = control.bridge.cancel.send(true);
    let _ = control.input.send(Input::Close);
    dial::cancel_prompts(&control.bridge);
    for slot in lock(&control.transfers).values() {
        let _ = slot.cancel.send(true);
    }
    lock(&control.forwards).clear();
}

async fn run_session(
    inner: Arc<Inner>,
    session_id: u64,
    host: Host,
    size: (u32, u32),
    bridge: Arc<Bridge>,
    input_rx: mpsc::UnboundedReceiver<Input>,
    output: Channel<InvokeResponseBody>,
    events: Channel<Value>,
) {
    emit(&events, json!({"type": "state", "state": "connecting"}));
    let prepared = control(&inner, session_id).map(|control| {
        (
            control.input.clone(),
            Arc::clone(&control.handle),
            Arc::clone(&control.cwd),
        )
    });
    let failure = match prepared {
        Ok((input_tx, handle, cwd)) => match open_shell(
            Arc::clone(&inner),
            &host,
            size,
            &bridge,
            input_tx,
            input_rx,
            &output,
            &events,
            handle,
            cwd,
        )
        .await
        {
            Ok(()) => {
                if *bridge.cancel.borrow() {
                    SshError::new("cancelled")
                } else {
                    SshError::new("disconnected")
                }
            }
            Err(error) => error,
        },
        Err(error) => error,
    };
    log_ssh("session", host.id, failure.code);
    emit(
        &events,
        json!({"type": "closed", "code": failure.code, "params": failure.params}),
    );
}

async fn open_shell(
    inner: Arc<Inner>,
    host: &Host,
    size: (u32, u32),
    bridge: &Arc<Bridge>,
    input_tx: mpsc::UnboundedSender<Input>,
    input_rx: mpsc::UnboundedReceiver<Input>,
    output: &Channel<InvokeResponseBody>,
    events: &Channel<Value>,
    handle_slot: Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>,
    cwd: Arc<Mutex<CwdState>>,
) -> Result<(), SshError> {
    let jump_hosts = {
        let store = lock(&inner.store);
        host.jumps
            .iter()
            .map(|id| store.get(*id))
            .collect::<Result<Vec<_>, _>>()?
    };
    let env = dial::DialEnv {
        store: Arc::clone(&inner.store),
        known: Arc::clone(&inner.known),
    };
    let dialed = dial::open(&env, host, &jump_hosts, bridge, events).await?;
    let dial::Dialed {
        target: handle,
        jumps,
    } = dialed;
    {
        let mut slot = handle_slot.lock().await;
        *slot = Some(handle);
    }
    let channel = {
        let slot = handle_slot.lock().await;
        let Some(handle) = slot.as_ref() else {
            return Err(SshError::new("disconnected"));
        };
        match handle.channel_open_session().await {
            Ok(channel) => channel,
            Err(_) if handle.is_closed() => return Err(SshError::new("disconnected")),
            Err(error) => return Err(dial::map_russh(&error)),
        }
    };
    // Echo stays off until the directory hook runs `stty echo`, so that line is not printed.
    let modes: &[(russh::Pty, u32)] = &[(russh::Pty::ECHO, 0)];
    channel
        .request_pty(false, &host.term, size.0, size.1, 0, 0, modes)
        .await
        .map_err(|error| dial::map_russh(&error))?;
    channel
        .request_shell(false)
        .await
        .map_err(|error| dial::map_russh(&error))?;
    if host.agent_forward {
        let _ = channel.agent_forward(false).await;
    }
    let nonce = grant::nonce();
    if let Some(dir) = &host.start_dir {
        if let Some(path) = normalize_absolute(dir) {
            if let Some(path) = lock(&cwd).set_absolute_start(&path) {
                emit_cwd(events, &path);
            }
        } else {
            lock(&cwd).set_relative_start(dir);
        }
    }
    if let Some(hook) = cwd_hook(&nonce).ok() {
        let command = if let Some(dir) = &host.start_dir {
            let quoted = shell_single_quote(dir)?;
            format!("cd -- {quoted}; {hook}")
        } else {
            hook
        };
        lock(&cwd).arm_hide(&format!("PIXO{nonce}"));
        if channel.data_bytes(command.into_bytes()).await.is_err() {
            lock(&cwd).disarm_hide();
        }
    }
    emit(events, json!({"type": "state", "state": "ready"}));
    let home_slot = Arc::clone(&handle_slot);
    let home_cwd = Arc::clone(&cwd);
    let home_events = events.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(sftp) = open_sftp(&home_slot).await else {
            return;
        };
        let Ok(home) = sftp.canonicalize(".").await else {
            let _ = sftp.close().await;
            return;
        };
        if let Some(path) = lock(&home_cwd).set_home(home) {
            emit_cwd(&home_events, &path);
        }
        let _ = sftp.close().await;
    });
    let (mut reader, writer) = channel.split();
    let write_task = tauri::async_runtime::spawn(write_loop(
        input_rx,
        writer,
        Arc::clone(&cwd),
        events.clone(),
    ));
    pump(&mut reader, output, bridge, &cwd, events).await;
    let _ = input_tx.send(Input::Close);
    if let Some(handle) = handle_slot.lock().await.take() {
        let _ = handle.disconnect(Disconnect::ByApplication, "", "").await;
    }
    for jump in jumps {
        let _ = jump.disconnect(Disconnect::ByApplication, "", "").await;
    }
    let _ = write_task.await;
    Ok(())
}

async fn open_sftp(
    slot: &AsyncMutex<Option<client::Handle<ClientHandler>>>,
) -> Result<russh_sftp::client::SftpSession, SshError> {
    let channel = {
        let guard = slot.lock().await;
        let Some(handle) = guard.as_ref() else {
            return Err(SshError::new("disconnected"));
        };
        match handle.channel_open_session().await {
            Ok(channel) => channel,
            Err(_) if handle.is_closed() => return Err(SshError::new("disconnected")),
            Err(_) => return Err(SshError::new("sftp_unavailable")),
        }
    };
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|_| SshError::new("sftp_unavailable"))?;
    let config = russh_sftp::client::Config {
        max_packet_len: 256 * 1024,
        max_concurrent_reads: 4,
        max_concurrent_writes: 4,
        max_write_packet_len: 256 * 1024,
        request_timeout_secs: 60,
    };
    russh_sftp::client::SftpSession::new_with_config(channel.into_stream(), config)
        .await
        .map_err(|_| SshError::new("sftp_unavailable"))
}

async fn run_upload(
    handle: Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>,
    cwd: Arc<Mutex<CwdState>>,
    paths: Vec<std::path::PathBuf>,
    task_id: u64,
    progress: Channel<Value>,
    mut cancel: watch::Receiver<bool>,
    gate: Arc<DecisionGate>,
) -> Result<(), SshError> {
    let nodes = tauri::async_runtime::spawn_blocking(move || transfer::collect_local(&paths))
        .await
        .map_err(|_| SshError::new("storage"))??;
    if *cancel.borrow() {
        return Err(SshError::new("cancelled"));
    }
    let sftp = open_sftp(&handle).await?;
    let target = transfer::resolve_target(&sftp, &cwd).await?;
    let mut files = 0usize;
    let mut total_bytes = 0u64;
    for node in &nodes {
        if let transfer::LocalNode::File { bytes, .. } = node {
            files += 1;
            total_bytes = total_bytes.saturating_add(*bytes);
        }
    }
    let _ = progress.send(json!({
        "type": "started",
        "taskId": task_id,
        "target": target,
        "files": files,
        "total": total_bytes,
    }));
    let result = transfer::send_all(
        &sftp,
        &target,
        &nodes,
        task_id,
        &progress,
        &mut cancel,
        gate.as_ref(),
    )
    .await;
    let _ = sftp.close().await;
    result
}

async fn run_download(
    handle: Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>,
    root: std::path::PathBuf,
    paths: Vec<String>,
    task_id: u64,
    progress: Channel<Value>,
    mut cancel: watch::Receiver<bool>,
    gate: Arc<DecisionGate>,
) -> Result<(), SshError> {
    let sftp = open_sftp(&handle).await?;
    let result = transfer::receive_all(
        &sftp,
        &root,
        &paths,
        task_id,
        &progress,
        &mut cancel,
        gate.as_ref(),
    )
    .await;
    let _ = sftp.close().await;
    result
}

async fn write_loop(
    mut input: mpsc::UnboundedReceiver<Input>,
    writer: russh::ChannelWriteHalf<Msg>,
    cwd: Arc<Mutex<CwdState>>,
    events: Channel<Value>,
) {
    while let Some(command) = input.recv().await {
        let failed = match command {
            Input::Write(data) => {
                let path = lock(&cwd).feed_input(&data);
                if let Some(path) = path {
                    emit_cwd(&events, &path);
                }
                writer.data_bytes(data).await.is_err()
            }
            Input::Interrupt => {
                let _ = lock(&cwd).feed_input(&[3]);
                let _ = writer.signal(Sig::INT).await;
                writer.data_bytes(vec![3]).await.is_err()
            }
            Input::Resize { cols, rows } => {
                let (cols, rows) = clamp_size(cols, rows);
                writer.window_change(cols, rows, 0, 0).await.is_err()
            }
            Input::Close => true,
        };
        if failed {
            break;
        }
    }
    let _ = writer.close().await;
}

async fn pump(
    reader: &mut russh::ChannelReadHalf,
    output: &Channel<InvokeResponseBody>,
    bridge: &Bridge,
    cwd: &Mutex<CwdState>,
    events: &Channel<Value>,
) {
    let mut pending = Vec::new();
    let mut flush_at: Option<tokio::time::Instant> = None;
    let mut cancel = bridge.cancel.subscribe();
    loop {
        if bridge.output.blocked() {
            // Bytes already pulled off the channel still have to reach the terminal,
            // or the matching ack never arrives and the reader stays paused.
            bridge.output.charge(emit_output(output, &mut pending));
            flush_at = None;
            if !wait_for_output_room(&bridge.output, &mut cancel).await {
                break;
            }
        }
        let flush_ready = flush_at.is_some();
        let sleep = tokio::time::sleep_until(
            flush_at.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(3600)),
        );
        tokio::pin!(sleep);
        tokio::select! {
            _ = cancel.wait_for(|cancelled| *cancelled) => break,
            message = reader.wait() => {
                match message {
                    Some(ChannelMsg::Data { data }) => {
                        let (visible, path) = {
                            let mut cwd = lock(cwd);
                            let path = cwd.feed_output(&data);
                            let visible = cwd.filter_output(&data);
                            (visible, path)
                        };
                        if let Some(path) = path {
                            emit_cwd(events, &path);
                        }
                        if visible.is_empty() {
                            continue;
                        }
                        if pending.is_empty() {
                            flush_at = Some(tokio::time::Instant::now() + OUTPUT_FLUSH);
                        }
                        pending.extend_from_slice(&visible);
                        if pending.len() >= OUTPUT_LIMIT {
                            bridge.output.charge(emit_output(output, &mut pending));
                            flush_at = None;
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        let visible = lock(cwd).filter_output(&data);
                        if visible.is_empty() {
                            continue;
                        }
                        if pending.is_empty() {
                            flush_at = Some(tokio::time::Instant::now() + OUTPUT_FLUSH);
                        }
                        pending.extend_from_slice(&visible);
                        if pending.len() >= OUTPUT_LIMIT {
                            bridge.output.charge(emit_output(output, &mut pending));
                            flush_at = None;
                        }
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | Some(ChannelMsg::ExitStatus { .. }) | None => {
                        bridge.output.charge(emit_output(output, &mut pending));
                        return;
                    }
                    _ => {}
                }
            }
            _ = &mut sleep, if flush_ready => {
                bridge.output.charge(emit_output(output, &mut pending));
                flush_at = None;
            }
        }
    }
    bridge.output.charge(emit_output(output, &mut pending));
}

fn transfer_failure_code(code: &'static str, session_cancelled: bool) -> &'static str {
    if code == "cancelled" && session_cancelled {
        "disconnected"
    } else {
        code
    }
}

fn report_transfer_failure(
    progress: &Channel<Value>,
    task_id: u64,
    error: SshError,
    session_cancelled: bool,
) {
    let code = transfer_failure_code(error.code, session_cancelled);
    log_ssh("transfer", 0, code);
    let _ = progress.send(json!({
        "type": "failed",
        "taskId": task_id,
        "code": code,
        "params": error.params,
    }));
}

fn emit(events: &Channel<Value>, value: Value) {
    let _ = events.send(value);
}

fn emit_cwd(events: &Channel<Value>, path: &str) {
    emit(events, json!({"type": "cwd", "path": path}));
}

fn emit_output(output: &Channel<InvokeResponseBody>, pending: &mut Vec<u8>) -> u64 {
    if pending.is_empty() {
        return 0;
    }
    let bytes = std::mem::take(pending);
    let sent = bytes.len() as u64;
    let _ = output.send(InvokeResponseBody::Raw(bytes));
    sent
}

async fn accept_forward(
    listener: TcpListener,
    handle: Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>,
    remote_host: String,
    remote_port: u32,
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
                let Ok((socket, peer)) = accepted else {
                    continue;
                };
                let handle = Arc::clone(&handle);
                let remote_host = remote_host.clone();
                let mut stop = cancel.clone();
                tauri::async_runtime::spawn(async move {
                    let channel = {
                        let slot = handle.lock().await;
                        let Some(session) = slot.as_ref() else {
                            return;
                        };
                        session
                            .channel_open_direct_tcpip(
                                remote_host,
                                remote_port,
                                peer.ip().to_string(),
                                u32::from(peer.port()),
                            )
                            .await
                            .ok()
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

fn clamp_size(cols: u32, rows: u32) -> (u32, u32) {
    (cols.clamp(1, 1000), rows.clamp(1, 400))
}

fn control(inner: &Inner, session_id: u64) -> Result<SessionControl, SshError> {
    lock(&inner.sessions)
        .get(&session_id)
        .cloned()
        .ok_or_else(|| SshError::new("not_found"))
}

fn tighten(path: &std::path::Path) {
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

fn storage(error: impl std::fmt::Display) -> SshError {
    eprintln!("ssh storage: {error}");
    SshError::new("storage")
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use super::transfer_failure_code;

    #[test]
    fn a_disconnect_is_not_reported_as_a_user_cancel() {
        assert_eq!(transfer_failure_code("cancelled", true), "disconnected");
        assert_eq!(transfer_failure_code("cancelled", false), "cancelled");
        assert_eq!(transfer_failure_code("timeout", true), "timeout");
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
