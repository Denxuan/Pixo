use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handler, Msg};
use russh::keys::{Algorithm, PrivateKeyWithHashAlg};
use russh::{ChannelMsg, Disconnect};
use serde_json::{Value, json};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpStream;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::{mpsc, oneshot, watch};
use zeroize::{Zeroize, Zeroizing};

use super::auth::{self, LoadedKey};
use super::cwd::CwdState;
use super::error::{SshError, log_ssh};
use super::grant;
use super::known_hosts::{Decision, KeyChange, KnownStore, user_known_hosts_path};
use super::model::{AuthKind, Host, HostDraft, SshStatus, normalize};
use super::remote::normalize_absolute;
use super::secret::{self, SecretKind};
use super::shell::{cwd_hook, shell_single_quote};
use super::store::Store;
use super::transfer::{self, DecisionGate, TransferSlot};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const PROMPT_TIMEOUT: Duration = Duration::from_secs(120);
const OUTPUT_FLUSH: Duration = Duration::from_millis(16);
const OUTPUT_LIMIT: usize = 64 * 1024;

enum Input {
    Write(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
}

struct SubmittedSecret {
    secret: String,
    remember: bool,
}

struct PromptSlot {
    id: u64,
    sender: oneshot::Sender<Option<SubmittedSecret>>,
}

struct Bridge {
    cancel: watch::Sender<bool>,
    hostkey: Mutex<Option<oneshot::Sender<bool>>>,
    prompt: Mutex<Option<PromptSlot>>,
    next_prompt: AtomicU64,
    failure: Mutex<Option<SshError>>,
}

#[derive(Clone)]
struct SessionControl {
    bridge: Arc<Bridge>,
    input: mpsc::UnboundedSender<Input>,
    handle: Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>,
    cwd: Arc<Mutex<CwdState>>,
    transfers: Arc<Mutex<HashMap<u64, TransferSlot>>>,
}

struct Inner {
    app: AppHandle,
    store: Mutex<Store>,
    known: Mutex<KnownStore>,
    sessions: Mutex<HashMap<u64, SessionControl>>,
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
            store: Mutex::new(store),
            known: Mutex::new(known),
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            next_task: AtomicU64::new(1),
        }),
    })
}

impl SshState {
    pub fn status(&self) -> Result<SshStatus, SshError> {
        Ok(self.status_of(lock(&self.inner.store).enabled()?))
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<SshStatus, SshError> {
        lock(&self.inner.store).set_enabled(enabled)?;
        if !enabled {
            self.disconnect_all();
        }
        let _ = self.inner.app.emit("ssh://enabled", enabled);
        Ok(self.status_of(enabled))
    }

    fn status_of(&self, enabled: bool) -> SshStatus {
        SshStatus {
            enabled,
            keyring_available: secret::available(),
        }
    }

    pub fn list(&self) -> Result<Vec<Host>, SshError> {
        lock(&self.inner.store).list()
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
        lock(&self.inner.store).save(&host, now_millis())
    }

    pub fn delete(&self, id: i64) -> Result<(), SshError> {
        let host = lock(&self.inner.store).get(id)?;
        if host.remember {
            secret::delete_host(id)?;
        }
        lock(&self.inner.store).delete(id)
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
        let (cancel, _) = watch::channel(false);
        let bridge = Arc::new(Bridge {
            cancel,
            hostkey: Mutex::new(None),
            prompt: Mutex::new(None),
            next_prompt: AtomicU64::new(1),
            failure: Mutex::new(None),
        });
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

    pub fn decide_host_key(&self, session_id: u64, accept: bool) -> Result<(), SshError> {
        let control = control(&self.inner, session_id)?;
        let sender = lock(&control.bridge.hostkey)
            .take()
            .ok_or_else(|| SshError::new("not_found"))?;
        let _ = sender.send(accept);
        Ok(())
    }

    pub fn answer(
        &self,
        session_id: u64,
        prompt_id: u64,
        secret: Option<String>,
        remember: bool,
        cancel: bool,
    ) -> Result<(), SshError> {
        if secret.as_ref().is_some_and(|value| value.len() > 1024) {
            return Err(SshError::new("invalid_input"));
        }
        let control = control(&self.inner, session_id)?;
        let mut slot = lock(&control.bridge.prompt);
        let Some(wait) = slot.take() else {
            return Err(SshError::new("not_found"));
        };
        if wait.id != prompt_id {
            *slot = Some(wait);
            return Err(SshError::new("invalid_input"));
        }
        let _ = wait.sender.send(if cancel {
            None
        } else {
            Some(SubmittedSecret {
                secret: secret.unwrap_or_default(),
                remember,
            })
        });
        Ok(())
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
        let control = control(&self.inner, session_id)?;
        let paths = grant::take(&token)?;
        let task_id = self.inner.next_task.fetch_add(1, Ordering::Relaxed);
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
                log_ssh("upload", 0, error.code);
                let _ = progress.send(json!({
                    "type": "failed",
                    "taskId": task_id,
                    "code": error.code,
                    "params": error.params,
                }));
            }
        });
        Ok(task_id)
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
    if let Some(sender) = lock(&control.bridge.hostkey).take() {
        let _ = sender.send(false);
    }
    if let Some(slot) = lock(&control.bridge.prompt).take() {
        let _ = slot.sender.send(None);
    }
    for slot in lock(&control.transfers).values() {
        let _ = slot.cancel.send(true);
    }
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
    let socket = match tokio::time::timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((host.host.as_str(), host.port)),
    )
    .await
    {
        Ok(Ok(socket)) => socket,
        Ok(Err(error)) => return Err(map_io(&error)),
        Err(_) => return Err(SshError::new("timeout")),
    };
    let _ = socket.set_nodelay(true);
    let mut config = client::Config::default();
    config.keepalive_interval = Some(Duration::from_secs(30));
    config.keepalive_max = 3;
    config.nodelay = true;
    let handler = ClientHandler {
        inner: Arc::clone(&inner),
        bridge: Arc::clone(bridge),
        events: events.clone(),
        host: host.host.clone(),
        port: host.port,
    };
    let mut handle = match client::connect_stream(Arc::new(config), socket, handler).await {
        Ok(handle) => handle,
        Err(error) => return Err(take_failure(bridge).unwrap_or_else(|| map_russh(&error.0))),
    };
    if let Some(error) = take_failure(bridge) {
        return Err(error);
    }
    if *bridge.cancel.borrow() {
        return Err(SshError::new("cancelled"));
    }
    emit(events, json!({"type": "state", "state": "authenticating"}));
    authenticate(&mut handle, &inner, host, bridge, events).await?;
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
            Err(error) => return Err(map_russh(&error)),
        }
    };
    // Echo stays off until the directory hook runs `stty echo`, so that line is not printed.
    let modes: &[(russh::Pty, u32)] = &[(russh::Pty::ECHO, 0)];
    channel
        .request_pty(false, &host.term, size.0, size.1, 0, 0, modes)
        .await
        .map_err(|error| map_russh(&error))?;
    channel
        .request_shell(false)
        .await
        .map_err(|error| map_russh(&error))?;
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

async fn authenticate(
    handle: &mut client::Handle<ClientHandler>,
    inner: &Inner,
    host: &Host,
    bridge: &Bridge,
    events: &Channel<Value>,
) -> Result<(), SshError> {
    match host.auth {
        AuthKind::Password => {
            if let Some(saved) = saved_secret(events, host, SecretKind::Password).await {
                if password_accepted(handle, &host.username, saved.as_str()).await? {
                    return Ok(());
                }
                forget_secret(events, host.id, SecretKind::Password).await;
            }
            let reply = prompt_reply(bridge, events, "password", host.remember).await?;
            if !password_accepted(handle, &host.username, reply.secret.as_str()).await? {
                return Err(SshError::new("auth_failed"));
            }
            keep_secret(
                inner,
                events,
                host.id,
                SecretKind::Password,
                reply.secret.as_str(),
                reply.remember,
            )
            .await;
        }
        AuthKind::Key => {
            let path = host.key_path.as_deref().unwrap_or("");
            let key = match auth::load_private_key(path, None) {
                LoadedKey::Ready(key) => key,
                LoadedKey::Encrypted => {
                    let mut unlocked = None;
                    if let Some(saved) = saved_secret(events, host, SecretKind::Passphrase).await {
                        match auth::load_private_key(path, Some(saved.as_str())) {
                            LoadedKey::Ready(key) => unlocked = Some(key),
                            LoadedKey::Encrypted | LoadedKey::Failed => {
                                forget_secret(events, host.id, SecretKind::Passphrase).await;
                            }
                        }
                    }
                    if let Some(key) = unlocked {
                        key
                    } else {
                        let reply =
                            prompt_reply(bridge, events, "passphrase", host.remember).await?;
                        match auth::load_private_key(path, Some(reply.secret.as_str())) {
                            LoadedKey::Ready(key) => {
                                keep_secret(
                                    inner,
                                    events,
                                    host.id,
                                    SecretKind::Passphrase,
                                    reply.secret.as_str(),
                                    reply.remember,
                                )
                                .await;
                                key
                            }
                            LoadedKey::Encrypted | LoadedKey::Failed => {
                                return Err(SshError::new("key_unreadable"));
                            }
                        }
                    }
                }
                LoadedKey::Failed => return Err(SshError::new("key_unreadable")),
            };
            let hash = match handle.best_supported_rsa_hash().await {
                Ok(value) => value.flatten(),
                Err(_) => None,
            };
            if matches!(key.algorithm(), Algorithm::Rsa { .. }) && hash.is_none() {
                return Err(SshError::new("auth_failed"));
            }
            let result = handle
                .authenticate_publickey(
                    &host.username,
                    PrivateKeyWithHashAlg::new(std::sync::Arc::new(key), hash),
                )
                .await
                .map_err(|error| map_russh(&error))?;
            if !result.success() {
                return Err(SshError::new("auth_failed"));
            }
        }
    }
    Ok(())
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
                            emit_output(output, &mut pending);
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
                            emit_output(output, &mut pending);
                            flush_at = None;
                        }
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | Some(ChannelMsg::ExitStatus { .. }) | None => {
                        emit_output(output, &mut pending);
                        return;
                    }
                    _ => {}
                }
            }
            _ = &mut sleep, if flush_ready => {
                emit_output(output, &mut pending);
                flush_at = None;
            }
        }
    }
    emit_output(output, &mut pending);
}

struct ClientHandler {
    inner: Arc<Inner>,
    bridge: Arc<Bridge>,
    events: Channel<Value>,
    host: String,
    port: u16,
}

impl Handler for ClientHandler {
    type Error = HandlerError;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        if *self.bridge.cancel.borrow() {
            set_failure(&self.bridge, SshError::new("cancelled"));
            return Ok(false);
        }
        emit(&self.events, json!({"type": "state", "state": "verifying"}));
        let key = server_public_key.public_key();
        let decision = lock(&self.inner.known)
            .decide(&self.host, self.port, &key)
            .map_err(|error| {
                set_failure(&self.bridge, error.clone());
                HandlerError(russh::Error::UnknownKey)
            })?;
        match decision {
            Decision::Trusted => Ok(true),
            Decision::Changed(change) => {
                emit(&self.events, hostkey_event(&change));
                set_failure(&self.bridge, change.into_error());
                Ok(false)
            }
            Decision::Unknown {
                algorithm,
                fingerprint,
            } => {
                emit(
                    &self.events,
                    json!({
                        "type": "hostkey",
                        "status": "unknown",
                        "algorithm": algorithm,
                        "fingerprint": fingerprint,
                    }),
                );
                let accept = wait_hostkey(&self.bridge).await;
                if *self.bridge.cancel.borrow() {
                    set_failure(&self.bridge, SshError::new("cancelled"));
                    return Ok(false);
                }
                if !accept {
                    set_failure(&self.bridge, SshError::new("hostkey_rejected"));
                    return Ok(false);
                }
                if let Err(error) = lock(&self.inner.known).trust(&self.host, self.port, &key) {
                    set_failure(&self.bridge, error);
                    return Ok(false);
                }
                Ok(true)
            }
        }
    }
}

#[derive(Debug)]
struct HandlerError(russh::Error);

impl From<russh::Error> for HandlerError {
    fn from(error: russh::Error) -> Self {
        Self(error)
    }
}

impl KeyChange {
    fn into_error(self) -> SshError {
        SshError::with_params(
            "hostkey_changed",
            json!({
                "algorithm": self.algorithm,
                "fingerprint": self.fingerprint,
                "previous": self.previous,
                "source": self.source,
                "revoked": self.revoked,
            }),
        )
    }
}

fn hostkey_event(change: &KeyChange) -> Value {
    json!({
        "type": "hostkey",
        "status": "changed",
        "algorithm": change.algorithm,
        "fingerprint": change.fingerprint,
        "previous": change.previous,
        "source": change.source,
        "revoked": change.revoked,
    })
}

async fn wait_hostkey(bridge: &Bridge) -> bool {
    let receiver = {
        let (sender, receiver) = oneshot::channel();
        *lock(&bridge.hostkey) = Some(sender);
        receiver
    };
    let mut cancel = bridge.cancel.subscribe();
    let accepted = tokio::select! {
        _ = cancel.wait_for(|cancelled| *cancelled) => false,
        result = receiver => result.unwrap_or(false),
        _ = tokio::time::sleep(PROMPT_TIMEOUT) => false,
    };
    if lock(&bridge.hostkey).as_ref().is_some() {
        *lock(&bridge.hostkey) = None;
    }
    accepted
}

struct PromptReply {
    secret: Zeroizing<String>,
    remember: bool,
}

async fn saved_secret(
    events: &Channel<Value>,
    host: &Host,
    kind: SecretKind,
) -> Option<Zeroizing<String>> {
    if !host.remember || !secret::available() {
        return None;
    }
    match blocking_secret(host.id, kind, SecretOp::Get).await {
        Ok(SecretOutcome::Value(secret)) => secret,
        Ok(_) => None,
        Err(error) => {
            emit_notice(events, error.code);
            None
        }
    }
}

async fn forget_secret(events: &Channel<Value>, host_id: i64, kind: SecretKind) {
    if let Err(error) = blocking_secret(host_id, kind, SecretOp::Delete).await {
        emit_notice(events, error.code);
    }
}

async fn keep_secret(
    inner: &Inner,
    events: &Channel<Value>,
    host_id: i64,
    kind: SecretKind,
    secret: &str,
    remember: bool,
) {
    let remembered = remember && secret::available() && !secret.is_empty();
    if remembered {
        let owned = secret.to_string();
        if let Err(error) = blocking_secret(host_id, kind, SecretOp::Set(owned)).await {
            let _ = lock(&inner.store).set_remember(host_id, false, now_millis());
            emit_notice(events, error.code);
            return;
        }
    } else if let Err(error) = blocking_secret(host_id, kind, SecretOp::Delete).await {
        emit_notice(events, error.code);
        return;
    }
    if let Err(error) = lock(&inner.store).set_remember(host_id, remembered, now_millis()) {
        emit_notice(events, error.code);
    }
}

enum SecretOp {
    Get,
    Set(String),
    Delete,
}

enum SecretOutcome {
    Value(Option<Zeroizing<String>>),
    Done,
}

async fn blocking_secret(
    host_id: i64,
    kind: SecretKind,
    operation: SecretOp,
) -> Result<SecretOutcome, SshError> {
    tauri::async_runtime::spawn_blocking(move || match operation {
        SecretOp::Get => secret::get(host_id, kind).map(SecretOutcome::Value),
        SecretOp::Set(mut secret) => {
            let result = secret::set(host_id, kind, &secret);
            secret.zeroize();
            result.map(|()| SecretOutcome::Done)
        }
        SecretOp::Delete => secret::delete(host_id, kind).map(|()| SecretOutcome::Done),
    })
    .await
    .map_err(|_| SshError::new("storage"))?
}

async fn password_accepted(
    handle: &mut client::Handle<ClientHandler>,
    username: &str,
    password: &str,
) -> Result<bool, SshError> {
    let result = handle
        .authenticate_password(username, password)
        .await
        .map_err(|error| map_russh(&error))?;
    Ok(result.success())
}

async fn prompt_reply(
    bridge: &Bridge,
    events: &Channel<Value>,
    kind: &str,
    remember: bool,
) -> Result<PromptReply, SshError> {
    if *bridge.cancel.borrow() {
        return Err(SshError::new("cancelled"));
    }
    let can_remember = secret::available();
    let remember = remember && can_remember;
    let id = bridge.next_prompt.fetch_add(1, Ordering::Relaxed);
    let receiver = {
        let (sender, receiver) = oneshot::channel();
        *lock(&bridge.prompt) = Some(PromptSlot { id, sender });
        receiver
    };
    emit(
        events,
        json!({
            "type": "prompt",
            "promptId": id,
            "kind": kind,
            "canRemember": can_remember,
            "remember": remember,
        }),
    );
    let mut cancel = bridge.cancel.subscribe();
    let answer = tokio::select! {
        _ = cancel.wait_for(|cancelled| *cancelled) => None,
        result = receiver => result.ok().flatten(),
        _ = tokio::time::sleep(PROMPT_TIMEOUT) => None,
    };
    {
        let mut slot = lock(&bridge.prompt);
        if slot.as_ref().is_some_and(|slot| slot.id == id) {
            *slot = None;
        }
    }
    if *bridge.cancel.borrow() {
        return Err(SshError::new("cancelled"));
    }
    let Some(answer) = answer else {
        return Err(SshError::new("cancelled"));
    };
    Ok(PromptReply {
        secret: Zeroizing::new(answer.secret),
        remember: answer.remember && can_remember,
    })
}

fn emit_notice(events: &Channel<Value>, code: &str) {
    emit(
        events,
        json!({"type": "notice", "code": code, "params": {}}),
    );
}

fn set_failure(bridge: &Bridge, error: SshError) {
    *lock(&bridge.failure) = Some(error);
}

fn take_failure(bridge: &Bridge) -> Option<SshError> {
    lock(&bridge.failure).take()
}

fn emit(events: &Channel<Value>, value: Value) {
    let _ = events.send(value);
}

fn emit_cwd(events: &Channel<Value>, path: &str) {
    emit(events, json!({"type": "cwd", "path": path}));
}

fn emit_output(output: &Channel<InvokeResponseBody>, pending: &mut Vec<u8>) {
    if pending.is_empty() {
        return;
    }
    let bytes = std::mem::take(pending);
    let _ = output.send(InvokeResponseBody::Raw(bytes));
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

fn map_io(error: &std::io::Error) -> SshError {
    if error.kind() == std::io::ErrorKind::TimedOut {
        return SshError::new("timeout");
    }
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("nodename")
        || text.contains("name or service")
        || text.contains("no address")
        || text.contains("temporary failure in name")
        || text.contains("name resolution")
    {
        SshError::new("resolve_failed")
    } else {
        SshError::new("connect_failed")
    }
}

fn map_russh(error: &russh::Error) -> SshError {
    match error {
        russh::Error::ConnectionTimeout
        | russh::Error::Elapsed(_)
        | russh::Error::InactivityTimeout
        | russh::Error::KeepaliveTimeout => SshError::new("timeout"),
        russh::Error::IO(io) => map_io(io),
        _ => SshError::new("connect_failed"),
    }
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

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
