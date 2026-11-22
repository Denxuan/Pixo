use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, AuthResult, Handler};
use russh::keys::{Algorithm, PrivateKeyWithHashAlg};
use russh::{ChannelOpenFailure, MethodKind};
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tokio::net::TcpStream;
use tokio::sync::{oneshot, watch};
use zeroize::{Zeroize, Zeroizing};

use super::auth::{self, LoadedKey};
use super::error::SshError;
use super::flow::OutputWindow;
use super::known_hosts::{Decision, KeyChange, KnownStore};
use super::model::{AuthKind, Host};
use super::secret::{self, SecretKind};
use super::store::Store;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const PROMPT_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) struct SubmittedSecret {
    pub(crate) secret: String,
    pub(crate) remember: bool,
}

struct PromptSlot {
    id: u64,
    sender: oneshot::Sender<Option<SubmittedSecret>>,
}

pub(crate) struct Bridge {
    pub(crate) cancel: watch::Sender<bool>,
    pub(crate) hostkey: Mutex<Option<oneshot::Sender<bool>>>,
    prompt: Mutex<Option<PromptSlot>>,
    next_prompt: AtomicU64,
    failure: Mutex<Option<SshError>>,
    pub(crate) output: OutputWindow,
}

impl Bridge {
    pub(crate) fn new() -> Arc<Self> {
        let (cancel, _) = watch::channel(false);
        Arc::new(Self {
            cancel,
            hostkey: Mutex::new(None),
            prompt: Mutex::new(None),
            next_prompt: AtomicU64::new(1),
            failure: Mutex::new(None),
            output: OutputWindow::new(),
        })
    }
}

#[derive(Clone)]
pub(crate) struct DialEnv {
    pub(crate) store: Arc<Mutex<Store>>,
    pub(crate) known: Arc<Mutex<KnownStore>>,
}

pub(crate) struct Dialed {
    pub(crate) target: client::Handle<ClientHandler>,
    pub(crate) jumps: Vec<client::Handle<ClientHandler>>,
}

pub(crate) fn decide_hostkey(bridge: &Bridge, accept: bool) -> Result<(), SshError> {
    let sender = lock(&bridge.hostkey)
        .take()
        .ok_or_else(|| SshError::new("not_found"))?;
    let _ = sender.send(accept);
    Ok(())
}

pub(crate) fn answer_prompt(
    bridge: &Bridge,
    prompt_id: u64,
    secret: Option<String>,
    remember: bool,
    cancel: bool,
) -> Result<(), SshError> {
    if secret.as_ref().is_some_and(|value| value.len() > 1024) {
        return Err(SshError::new("invalid_input"));
    }
    let mut slot = lock(&bridge.prompt);
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

pub(crate) fn cancel_prompts(bridge: &Bridge) {
    if let Some(sender) = lock(&bridge.hostkey).take() {
        let _ = sender.send(false);
    }
    if let Some(slot) = lock(&bridge.prompt).take() {
        let _ = slot.sender.send(None);
    }
}

pub(crate) async fn open(
    env: &DialEnv,
    target: &Host,
    jumps: &[Host],
    bridge: &Arc<Bridge>,
    events: &Channel<Value>,
) -> Result<Dialed, SshError> {
    let mut previous = None;
    let mut kept = Vec::new();
    for (index, hop) in jumps.iter().chain(std::iter::once(target)).enumerate() {
        let handle = connect_hop(env, hop, previous.as_ref(), bridge, events).await?;
        if let Some(prior) = previous.take() {
            kept.push(prior);
        }
        previous = Some(handle);
        if index == jumps.len() {
            break;
        }
    }
    Ok(Dialed {
        target: previous.ok_or_else(|| SshError::new("disconnected"))?,
        jumps: kept,
    })
}

async fn connect_hop(
    env: &DialEnv,
    hop: &Host,
    via: Option<&client::Handle<ClientHandler>>,
    bridge: &Arc<Bridge>,
    events: &Channel<Value>,
) -> Result<client::Handle<ClientHandler>, SshError> {
    match via {
        None => {
            let socket = match tokio::time::timeout(
                CONNECT_TIMEOUT,
                TcpStream::connect((hop.host.as_str(), hop.port)),
            )
            .await
            {
                Ok(Ok(socket)) => socket,
                Ok(Err(error)) => return Err(map_io(&error)),
                Err(_) => return Err(SshError::new("timeout")),
            };
            let _ = socket.set_nodelay(true);
            finish_connect(env, hop, socket, bridge, events).await
        }
        Some(via) => {
            let channel = match tokio::time::timeout(
                CONNECT_TIMEOUT,
                via.channel_open_direct_tcpip(hop.host.as_str(), hop.port as u32, "127.0.0.1", 0),
            )
            .await
            {
                Ok(Ok(channel)) => channel,
                Ok(Err(error)) => return Err(map_russh(&error)),
                Err(_) => return Err(SshError::new("timeout")),
            };
            finish_connect(env, hop, channel.into_stream(), bridge, events).await
        }
    }
}

async fn finish_connect<S>(
    env: &DialEnv,
    hop: &Host,
    stream: S,
    bridge: &Arc<Bridge>,
    events: &Channel<Value>,
) -> Result<client::Handle<ClientHandler>, SshError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let keepalive = lock(&env.store).keepalive().unwrap_or(30);
    let mut config = client::Config::default();
    config.keepalive_interval = if keepalive == 0 {
        None
    } else {
        Some(Duration::from_secs(u64::from(keepalive)))
    };
    config.keepalive_max = 3;
    config.nodelay = true;
    let handler = ClientHandler {
        env: env.clone(),
        bridge: Arc::clone(bridge),
        events: events.clone(),
        host: hop.host.clone(),
        port: hop.port,
        forward_agent: hop.agent_forward,
    };
    let mut handle = match client::connect_stream(Arc::new(config), stream, handler).await {
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
    authenticate(&mut handle, env, hop, bridge, events).await?;
    Ok(handle)
}

async fn authenticate(
    handle: &mut client::Handle<ClientHandler>,
    env: &DialEnv,
    host: &Host,
    bridge: &Bridge,
    events: &Channel<Value>,
) -> Result<(), SshError> {
    match host.auth {
        AuthKind::Password => password_auth(handle, env, host, bridge, events).await?,
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
                        let reply = prompt_reply(
                            bridge,
                            events,
                            "passphrase",
                            true,
                            host.remember,
                            &host.name,
                            "",
                            "",
                            "",
                            false,
                        )
                        .await?;
                        match auth::load_private_key(path, Some(reply.secret.as_str())) {
                            LoadedKey::Ready(key) => {
                                keep_secret(
                                    env,
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
            continue_auth(handle, env, host, bridge, events, result, false).await?;
        }
        AuthKind::Agent => {
            let hash = match handle.best_supported_rsa_hash().await {
                Ok(value) => value.flatten(),
                Err(_) => None,
            };
            let agent = agent_result(handle, host, hash).await;
            let result = if agent.success() {
                agent
            } else {
                let files = default_key_result(handle, host, hash).await?;
                if files.success() || wants_keyboard(&files) || !wants_keyboard(&agent) {
                    files
                } else {
                    agent
                }
            };
            continue_auth(handle, env, host, bridge, events, result, false).await?;
        }
    }
    Ok(())
}

async fn password_auth(
    handle: &mut client::Handle<ClientHandler>,
    env: &DialEnv,
    host: &Host,
    bridge: &Bridge,
    events: &Channel<Value>,
) -> Result<(), SshError> {
    match handle.authenticate_none(host.username.as_str()).await {
        Ok(result) if result.success() => return Ok(()),
        Ok(result)
            if wants_keyboard(&result) && !offers_method(&result, MethodKind::Password) =>
        {
            if keyboard_interactive(handle, env, host, bridge, events).await? {
                return Ok(());
            }
            return Err(SshError::new("auth_failed"));
        }
        Ok(_) => {}
        Err(error) if handle.is_closed() => return Err(map_russh(&error)),
        Err(_) => {}
    }
    if let Some(saved) = saved_secret(events, host, SecretKind::Password).await {
        let result = password_result(handle, &host.username, saved.as_str()).await?;
        if result.success() {
            return Ok(());
        }
        forget_secret(events, host.id, SecretKind::Password).await;
        if wants_keyboard(&result) && !offers_method(&result, MethodKind::Password) {
            return continue_auth(handle, env, host, bridge, events, result, true).await;
        }
    }
    let reply = prompt_reply(
        bridge,
        events,
        "password",
        true,
        host.remember,
        &host.name,
        "",
        "",
        "",
        false,
    )
    .await?;
    let result = password_result(handle, &host.username, reply.secret.as_str()).await?;
    if result.success() {
        keep_secret(
            env,
            events,
            host.id,
            SecretKind::Password,
            reply.secret.as_str(),
            reply.remember,
        )
        .await;
        return Ok(());
    }
    continue_auth(handle, env, host, bridge, events, result, true).await
}

async fn continue_auth(
    handle: &mut client::Handle<ClientHandler>,
    env: &DialEnv,
    host: &Host,
    bridge: &Bridge,
    events: &Channel<Value>,
    result: AuthResult,
    password_fallback: bool,
) -> Result<(), SshError> {
    if result.success() {
        return Ok(());
    }
    let follow = match &result {
        AuthResult::Failure { partial_success, .. } => {
            wants_keyboard(&result) && (*partial_success || password_fallback)
        }
        AuthResult::Success => false,
    };
    if follow && keyboard_interactive(handle, env, host, bridge, events).await? {
        return Ok(());
    }
    Err(SshError::new("auth_failed"))
}

async fn keyboard_interactive(
    handle: &mut client::Handle<ClientHandler>,
    env: &DialEnv,
    host: &Host,
    bridge: &Bridge,
    events: &Channel<Value>,
) -> Result<bool, SshError> {
    let mut step = handle
        .authenticate_keyboard_interactive_start(host.username.as_str(), Option::<String>::None)
        .await
        .map_err(|error| map_russh(&error))?;
    let mut remembered: Option<Zeroizing<String>> = None;
    let mut remember_flag = false;
    for _ in 0..8 {
        match step {
            client::KeyboardInteractiveAuthResponse::Success => {
                if let Some(secret) = remembered.as_ref() {
                    keep_secret(
                        env,
                        events,
                        host.id,
                        SecretKind::Password,
                        secret.as_str(),
                        remember_flag,
                    )
                    .await;
                }
                return Ok(true);
            }
            client::KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            client::KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                if prompts.len() > 8 {
                    return Err(SshError::new("auth_failed"));
                }
                if prompts.is_empty() {
                    step = handle
                        .authenticate_keyboard_interactive_respond(Vec::new())
                        .await
                        .map_err(|error| map_russh(&error))?;
                    continue;
                }
                let mut answers = Vec::with_capacity(prompts.len());
                let heading = visible_prompt(&name);
                let detail = visible_prompt(&instructions);
                for (index, prompt) in prompts.iter().enumerate() {
                    let label = visible_prompt(&prompt.prompt);
                    let allow_remember = prompts.len() == 1
                        && !prompt.echo
                        && matches!(host.auth, AuthKind::Password)
                        && password_like(&label);
                    let reply = prompt_reply(
                        bridge,
                        events,
                        "keyboard",
                        allow_remember,
                        host.remember,
                        &host.name,
                        if index == 0 { heading.as_str() } else { "" },
                        if index == 0 { detail.as_str() } else { "" },
                        &label,
                        prompt.echo,
                    )
                    .await?;
                    if reply.secret.len() > 4096 {
                        return Err(SshError::new("invalid_input"));
                    }
                    if allow_remember {
                        remembered = Some(Zeroizing::new(reply.secret.to_string()));
                        remember_flag = reply.remember;
                    }
                    answers.push(reply.secret.to_string());
                }
                step = handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(|error| map_russh(&error))?;
            }
        }
    }
    Err(SshError::new("auth_failed"))
}

fn wants_keyboard(result: &AuthResult) -> bool {
    offers_method(result, MethodKind::KeyboardInteractive)
}

fn offers_method(result: &AuthResult, method: MethodKind) -> bool {
    let AuthResult::Failure {
        remaining_methods, ..
    } = result
    else {
        return false;
    };
    format!("{remaining_methods:?}").contains(&format!("{method:?}"))
}

fn password_like(text: &str) -> bool {
    let lower = text.to_lowercase();
    let secret = lower.contains("password")
        || lower.contains("passphrase")
        || text.contains("密码")
        || text.contains("口令");
    let one_time = lower.contains("code")
        || lower.contains("otp")
        || lower.contains("verification")
        || lower.contains("one-time")
        || lower.contains("token")
        || text.contains("验证")
        || text.contains("一次性");
    secret && !one_time
}

fn visible_prompt(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if out.chars().count() >= 200 {
            break;
        }
        if ch == '\n' || ch == '\r' || ch == '\t' {
            out.push(' ');
        } else if !ch.is_control() {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

fn auth_failure() -> AuthResult {
    AuthResult::Failure {
        remaining_methods: russh::MethodSet::empty(),
        partial_success: false,
    }
}

async fn agent_result(
    handle: &mut client::Handle<ClientHandler>,
    host: &Host,
    hash: Option<russh::keys::HashAlg>,
) -> AuthResult {
    let Ok(mut client) = super::agent::open().await else {
        return auth_failure();
    };
    if let Some(binding) = handle.agent_session_binding() {
        if client.bind_session(binding, false).await.is_err() {
            let Ok(fresh) = super::agent::open().await else {
                return auth_failure();
            };
            client = fresh;
        }
    }
    let Ok(keys) = client.request_identities().await else {
        return auth_failure();
    };
    let mut last = auth_failure();
    for identity in keys {
        let rsa = match &identity {
            russh::keys::agent::AgentIdentity::PublicKey { key, .. } => {
                matches!(key.algorithm(), Algorithm::Rsa { .. })
            }
            russh::keys::agent::AgentIdentity::Certificate { certificate, .. } => {
                matches!(certificate.public_key().algorithm(), Algorithm::Rsa { .. })
            }
        };
        let key_hash = if rsa { hash } else { None };
        if rsa && key_hash.is_none() {
            continue;
        }
        let accepted = match identity {
            russh::keys::agent::AgentIdentity::Certificate { certificate, .. } => handle
                .authenticate_certificate_with(host.username.as_str(), certificate, key_hash, &mut client)
                .await,
            russh::keys::agent::AgentIdentity::PublicKey { key, .. } => handle
                .authenticate_publickey_with(host.username.as_str(), key, key_hash, &mut client)
                .await,
        };
        match accepted {
            Ok(result) if result.success() => return result,
            Ok(result) => last = result,
            Err(_) => {}
        }
    }
    last
}

async fn default_key_result(
    handle: &mut client::Handle<ClientHandler>,
    host: &Host,
    hash: Option<russh::keys::HashAlg>,
) -> Result<AuthResult, SshError> {
    let mut last = auth_failure();
    for path in auth::default_identity_files() {
        let key = match auth::load_private_key(&path.to_string_lossy(), None) {
            LoadedKey::Ready(key) => key,
            LoadedKey::Encrypted | LoadedKey::Failed => continue,
        };
        let rsa = matches!(key.algorithm(), Algorithm::Rsa { .. });
        if rsa && hash.is_none() {
            continue;
        }
        let result = handle
            .authenticate_publickey(
                &host.username,
                PrivateKeyWithHashAlg::new(std::sync::Arc::new(key), hash),
            )
            .await
            .map_err(|error| map_russh(&error))?;
        if result.success() {
            return Ok(result);
        }
        last = result;
    }
    Ok(last)
}

pub(crate) struct ClientHandler {
    env: DialEnv,
    bridge: Arc<Bridge>,
    events: Channel<Value>,
    host: String,
    port: u16,
    forward_agent: bool,
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
        let decision = lock(&self.env.known)
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
                if let Err(error) = lock(&self.env.known).trust(&self.host, self.port, &key) {
                    set_failure(&self.bridge, error);
                    return Ok(false);
                }
                Ok(true)
            }
        }
    }

    async fn server_channel_open_agent_forward(
        &mut self,
        channel: russh::Channel<russh::client::Msg>,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if !self.forward_agent {
            reply
                .reject(ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        }
        reply.accept().await;
        tauri::async_runtime::spawn(super::agent::relay(channel));
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct HandlerError(russh::Error);

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
    env: &DialEnv,
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
            let _ = lock(&env.store).set_remember(host_id, false, now_millis());
            emit_notice(events, error.code);
            return;
        }
    } else if let Err(error) = blocking_secret(host_id, kind, SecretOp::Delete).await {
        emit_notice(events, error.code);
        return;
    }
    if let Err(error) = lock(&env.store).set_remember(host_id, remembered, now_millis()) {
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

async fn password_result(
    handle: &mut client::Handle<ClientHandler>,
    username: &str,
    password: &str,
) -> Result<AuthResult, SshError> {
    handle
        .authenticate_password(username, password)
        .await
        .map_err(|error| map_russh(&error))
}

async fn prompt_reply(
    bridge: &Bridge,
    events: &Channel<Value>,
    kind: &str,
    allow_remember: bool,
    remember: bool,
    host_name: &str,
    title: &str,
    instructions: &str,
    label: &str,
    echo: bool,
) -> Result<PromptReply, SshError> {
    if *bridge.cancel.borrow() {
        return Err(SshError::new("cancelled"));
    }
    let can_remember = allow_remember && secret::available();
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
            "hostName": host_name,
            "title": title,
            "instructions": instructions,
            "label": label,
            "echo": echo,
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
    emit(events, json!({"type": "notice", "code": code, "params": {}}));
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

pub(crate) fn map_io(error: &std::io::Error) -> SshError {
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

pub(crate) fn map_russh(error: &russh::Error) -> SshError {
    match error {
        russh::Error::ConnectionTimeout
        | russh::Error::Elapsed(_)
        | russh::Error::InactivityTimeout
        | russh::Error::KeepaliveTimeout => SshError::new("timeout"),
        russh::Error::IO(io) => map_io(io),
        _ => SshError::new("connect_failed"),
    }
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
    use russh::{MethodKind, MethodSet};

    use super::{auth_failure, offers_method, password_like, wants_keyboard, AuthResult};

    #[test]
    fn keyboard_interactive_is_detected_from_the_server_list() {
        let mut methods = MethodSet::empty();
        methods.push(MethodKind::KeyboardInteractive);
        let result = AuthResult::Failure {
            remaining_methods: methods,
            partial_success: true,
        };
        assert!(wants_keyboard(&result));
        assert!(!offers_method(&result, MethodKind::Password));
        assert!(!wants_keyboard(&auth_failure()));
    }

    #[test]
    fn one_time_prompts_are_not_stored() {
        assert!(password_like("Password:"));
        assert!(password_like("密码"));
        assert!(!password_like("Verification code:"));
        assert!(!password_like(""));
    }
}
