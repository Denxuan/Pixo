use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, FileType, OpenFlags, StatusCode};
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{oneshot, watch};

use super::cwd::CwdState;
use super::error::SshError;
use super::remote::{
    join_remote, normalize_absolute, parent_remote, renamed_file, valid_component,
};

const CHUNK: usize = 256 * 1024;
const MAX_ROOTS: usize = 500;
const MAX_FILES: usize = 20_000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Overwrite,
    Rename,
    Skip,
}

pub struct Choice {
    pub policy: Policy,
    pub apply_all: bool,
}

pub struct DecisionGate {
    pending: Mutex<Option<oneshot::Sender<Choice>>>,
    saved: Mutex<Option<Policy>>,
}

impl Default for DecisionGate {
    fn default() -> Self {
        Self {
            pending: Mutex::new(None),
            saved: Mutex::new(None),
        }
    }
}

impl DecisionGate {
    pub fn answer(&self, policy: Policy, apply_all: bool) -> Result<(), SshError> {
        let sender = lock(&self.pending)
            .take()
            .ok_or_else(|| SshError::new("not_found"))?;
        sender
            .send(Choice { policy, apply_all })
            .map_err(|_| SshError::new("not_found"))
    }

    async fn choose_for(
        &self,
        progress: &Channel<Value>,
        task_id: u64,
        name: &str,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<Policy, SshError> {
        if let Some(policy) = *lock(&self.saved) {
            return Ok(policy);
        }
        let _ = progress.send(json!({
            "type": "conflict",
            "taskId": task_id,
            "name": name,
        }));
        let (sender, receiver) = oneshot::channel();
        *lock(&self.pending) = Some(sender);
        tokio::select! {
            choice = receiver => {
                let choice = choice.map_err(|_| SshError::new("cancelled"))?;
                if choice.apply_all {
                    *lock(&self.saved) = Some(choice.policy);
                }
                Ok(choice.policy)
            }
            _ = cancel.wait_for(|value| *value) => {
                lock(&self.pending).take();
                Err(SshError::new("cancelled"))
            }
            _ = tokio::time::sleep(Duration::from_secs(120)) => {
                lock(&self.pending).take();
                Err(SshError::new("cancelled"))
            }
        }
    }
}

pub struct TransferSlot {
    pub cancel: watch::Sender<bool>,
    pub gate: Arc<DecisionGate>,
}

pub fn parse_policy(value: &str) -> Result<Policy, SshError> {
    match value {
        "overwrite" => Ok(Policy::Overwrite),
        "rename" => Ok(Policy::Rename),
        "skip" => Ok(Policy::Skip),
        _ => Err(SshError::new("invalid_input")),
    }
}

pub enum LocalNode {
    Directory {
        relative: String,
    },
    File {
        relative: String,
        source: PathBuf,
        bytes: u64,
    },
    Skip {
        name: String,
    },
    Failed {
        name: String,
        code: &'static str,
    },
}

pub fn collect_local(roots: &[PathBuf]) -> Result<Vec<LocalNode>, SshError> {
    collect_limited(roots, MAX_FILES)
}

pub fn collect_limited(roots: &[PathBuf], max_files: usize) -> Result<Vec<LocalNode>, SshError> {
    if roots.is_empty() {
        return Err(SshError::new("invalid_input"));
    }
    if roots.len() > MAX_ROOTS {
        return Err(SshError::new("too_many"));
    }
    let mut nodes = Vec::new();
    let mut files = 0usize;
    for root in roots {
        let Some(name) = root.file_name() else {
            return Err(SshError::new("invalid_input"));
        };
        match component(name) {
            Ok(relative) => walk(root, &relative, 0, max_files, &mut files, &mut nodes)?,
            Err(name) => nodes.push(LocalNode::Skip { name }),
        }
    }
    if nodes.is_empty() {
        return Err(SshError::new("invalid_input"));
    }
    Ok(nodes)
}

pub async fn resolve_target(sftp: &SftpSession, cwd: &Mutex<CwdState>) -> Result<String, SshError> {
    let current = lock(cwd).path();
    if let Some(path) = current {
        return match sftp.canonicalize(path).await {
            Ok(canonical) => Ok(canonical),
            Err(error) if is_missing(&error) => Err(SshError::new("remote_missing")),
            Err(error) => Err(map_sftp(&error)),
        };
    }
    match sftp.canonicalize(".").await {
        Ok(home) => {
            let visible = lock(cwd).set_home(home.clone()).unwrap_or(home);
            Ok(visible)
        }
        Err(error) => Err(map_sftp(&error)),
    }
}

pub async fn send_all(
    sftp: &SftpSession,
    target: &str,
    nodes: &[LocalNode],
    task_id: u64,
    progress: &Channel<Value>,
    cancel: &mut watch::Receiver<bool>,
    gate: &DecisionGate,
) -> Result<(), SshError> {
    let mut uploaded = 0u64;
    let mut skipped = 0u64;
    let mut failed = 0u64;
    let mut sent = 0u64;
    let total_bytes = nodes.iter().fold(0u64, |sum, node| match node {
        LocalNode::File { bytes, .. } => sum.saturating_add(*bytes),
        _ => sum,
    });
    let mut blocked: Vec<(String, &'static str)> = Vec::new();
    for node in nodes {
        if *cancel.borrow() {
            return Err(SshError::new("cancelled"));
        }
        let relative = node_relative(node);
        if let Some(code) = blocked_code(&blocked, relative) {
            failed += 1;
            emit_file(progress, task_id, leaf(relative), "failed", Some(code));
            continue;
        }
        match upload_node(
            sftp,
            target,
            node,
            task_id,
            progress,
            cancel,
            gate,
            &mut sent,
            total_bytes,
        )
        .await
        {
            Ok(Item::Uploaded) => uploaded += 1,
            Ok(Item::Renamed) => uploaded += 1,
            Ok(Item::Skipped) => skipped += 1,
            Err(error) if error.code == "cancelled" => return Err(error),
            Err(error) => {
                failed += 1;
                emit_file(
                    progress,
                    task_id,
                    leaf(relative),
                    "failed",
                    Some(error.code),
                );
                if matches!(node, LocalNode::Directory { .. }) {
                    blocked.push((relative.to_string(), error.code));
                }
            }
        }
    }
    let _ = progress.send(json!({
        "type": "finished",
        "taskId": task_id,
        "uploaded": uploaded,
        "skipped": skipped,
        "failed": failed,
        "target": target,
    }));
    Ok(())
}

enum Item {
    Uploaded,
    Renamed,
    Skipped,
}

async fn upload_node(
    sftp: &SftpSession,
    target: &str,
    node: &LocalNode,
    task_id: u64,
    progress: &Channel<Value>,
    cancel: &mut watch::Receiver<bool>,
    gate: &DecisionGate,
    sent: &mut u64,
    total_bytes: u64,
) -> Result<Item, SshError> {
    match node {
        LocalNode::Skip { name } => {
            emit_file(progress, task_id, name, "skipped", None);
            Ok(Item::Skipped)
        }
        LocalNode::Failed { name, code } => Err(SshError::new(code).or_name(name)),
        LocalNode::Directory { relative } => {
            let remote =
                join_remote(target, relative).ok_or_else(|| SshError::new("invalid_input"))?;
            match remote_kind(sftp, &remote).await? {
                RemoteKind::Dir => {
                    emit_file(progress, task_id, leaf(relative), "done", None);
                    Ok(Item::Uploaded)
                }
                RemoteKind::Missing => {
                    sftp.create_dir(&remote)
                        .await
                        .map_err(|error| map_sftp(&error))?;
                    emit_file(progress, task_id, leaf(relative), "done", None);
                    Ok(Item::Uploaded)
                }
                RemoteKind::File | RemoteKind::Other => Err(SshError::new("transfer_failed")),
            }
        }
        LocalNode::File {
            relative,
            source,
            bytes,
        } => {
            let remote =
                join_remote(target, relative).ok_or_else(|| SshError::new("invalid_input"))?;
            let name = leaf(relative);
            let destination = match remote_kind(sftp, &remote).await? {
                RemoteKind::Missing => remote,
                RemoteKind::Dir | RemoteKind::Other => {
                    return Err(SshError::new("transfer_failed"));
                }
                RemoteKind::File => match gate.choose_for(progress, task_id, name, cancel).await? {
                    Policy::Skip => {
                        *sent = sent.saturating_add(*bytes);
                        emit_progress(progress, task_id, name, *sent, total_bytes);
                        emit_file(progress, task_id, name, "skipped", None);
                        return Ok(Item::Skipped);
                    }
                    Policy::Rename => free_name(sftp, &remote).await?,
                    Policy::Overwrite => remote,
                },
            };
            let renamed = destination != remote_join_original(target, relative);
            write_part(
                sftp,
                source,
                &destination,
                name,
                task_id,
                progress,
                cancel,
                sent,
                total_bytes,
            )
            .await?;
            if remote_kind(sftp, &destination).await? == RemoteKind::File
                && destination_matches_existing(&destination, target, relative)
            {
                replace_existing(sftp, &format!("{destination}.pixo-part"), &destination).await?;
            } else {
                finish_rename(
                    sftp,
                    &format!("{destination}.pixo-part"),
                    &destination,
                    false,
                )
                .await?;
            }
            if let Some(attrs) = mtime_attrs(source) {
                let _ = sftp.set_metadata(&destination, attrs).await;
            }
            emit_file(
                progress,
                task_id,
                name,
                if renamed { "renamed" } else { "done" },
                None,
            );
            Ok(if renamed {
                Item::Renamed
            } else {
                Item::Uploaded
            })
        }
    }
}

fn remote_join_original(target: &str, relative: &str) -> String {
    join_remote(target, relative).unwrap_or_default()
}

fn destination_matches_existing(destination: &str, target: &str, relative: &str) -> bool {
    remote_join_original(target, relative) == destination
}

async fn free_name(sftp: &SftpSession, remote: &str) -> Result<String, SshError> {
    let parent = parent_remote(remote).ok_or_else(|| SshError::new("invalid_input"))?;
    let name = remote.rsplit('/').next().unwrap_or(remote);
    for index in 1..=999 {
        let Some(candidate) = renamed_file(name, index) else {
            break;
        };
        let path =
            join_remote(&parent, &candidate).ok_or_else(|| SshError::new("invalid_input"))?;
        if remote_kind(sftp, &path).await? == RemoteKind::Missing {
            return Ok(path);
        }
    }
    Err(SshError::new("transfer_failed"))
}

async fn replace_existing(
    sftp: &SftpSession,
    part: &str,
    destination: &str,
) -> Result<(), SshError> {
    match sftp.rename(part, destination).await {
        Ok(()) => Ok(()),
        Err(_) => match remote_kind(sftp, destination).await? {
            RemoteKind::File => {
                sftp.remove_file(destination)
                    .await
                    .map_err(|error| map_sftp(&error))?;
                sftp.rename(part, destination)
                    .await
                    .map_err(|error| map_sftp(&error))?;
                Ok(())
            }
            _ => {
                let _ = sftp.remove_file(part).await;
                Err(SshError::new("transfer_failed"))
            }
        },
    }
}

async fn finish_rename(
    sftp: &SftpSession,
    part: &str,
    destination: &str,
    overwrite: bool,
) -> Result<(), SshError> {
    if overwrite {
        return replace_existing(sftp, part, destination).await;
    }
    if let Err(error) = sftp.rename(part, destination).await {
        let _ = sftp.remove_file(part).await;
        return Err(map_sftp(&error));
    }
    Ok(())
}

async fn write_part(
    sftp: &SftpSession,
    source: &Path,
    destination: &str,
    name: &str,
    task_id: u64,
    progress: &Channel<Value>,
    cancel: &watch::Receiver<bool>,
    sent: &mut u64,
    total_bytes: u64,
) -> Result<(), SshError> {
    let part = format!("{destination}.pixo-part");
    match remote_kind(sftp, &part).await? {
        RemoteKind::Missing => {}
        RemoteKind::File | RemoteKind::Other => {
            sftp.remove_file(&part)
                .await
                .map_err(|error| map_sftp(&error))?;
        }
        RemoteKind::Dir => return Err(SshError::new("transfer_failed")),
    }
    let mut local = tokio::fs::File::open(source)
        .await
        .map_err(|error| map_local_io(&error))?;
    let mut remote = sftp
        .open_with_flags(
            &part,
            OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE,
        )
        .await
        .map_err(|error| map_sftp(&error))?;
    let mut buffer = vec![0u8; CHUNK];
    let mut reported = Instant::now() - Duration::from_secs(1);
    emit_progress(progress, task_id, name, *sent, total_bytes);
    let result = async {
        loop {
            if *cancel.borrow() {
                return Err(SshError::new("cancelled"));
            }
            let read = local
                .read(&mut buffer)
                .await
                .map_err(|error| map_local_io(&error))?;
            if read == 0 {
                break;
            }
            remote
                .write_all(&buffer[..read])
                .await
                .map_err(|error| map_remote_io(&error))?;
            *sent = sent.saturating_add(read as u64);
            if reported.elapsed() >= Duration::from_millis(200) {
                emit_progress(progress, task_id, name, *sent, total_bytes);
                reported = Instant::now();
            }
        }
        remote
            .close()
            .await
            .map_err(|error| map_remote_io(&error))?;
        emit_progress(progress, task_id, name, *sent, total_bytes);
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = sftp.remove_file(&part).await;
    }
    result
}

fn mtime_attrs(path: &Path) -> Option<FileAttributes> {
    let modified = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    let secs = u32::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_secs()).ok()?;
    Some(FileAttributes {
        atime: Some(secs),
        mtime: Some(secs),
        ..FileAttributes::default()
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RemoteKind {
    Missing,
    File,
    Dir,
    Other,
}

async fn remote_kind(sftp: &SftpSession, path: &str) -> Result<RemoteKind, SshError> {
    match sftp.symlink_metadata(path).await {
        Ok(meta) => Ok(match meta.file_type() {
            FileType::Dir => RemoteKind::Dir,
            FileType::File => RemoteKind::File,
            _ => RemoteKind::Other,
        }),
        Err(error) if is_missing(&error) => Ok(RemoteKind::Missing),
        Err(error) => Err(map_sftp(&error)),
    }
}

fn walk(
    path: &Path,
    relative: &str,
    depth: usize,
    max_files: usize,
    files: &mut usize,
    nodes: &mut Vec<LocalNode>,
) -> Result<(), SshError> {
    if depth > 64 {
        nodes.push(LocalNode::Failed {
            name: relative.to_string(),
            code: "transfer_failed",
        });
        return Ok(());
    }
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) => {
            nodes.push(LocalNode::Failed {
                name: relative.to_string(),
                code: local_code(&error),
            });
            return Ok(());
        }
    };
    if meta.file_type().is_symlink() {
        nodes.push(LocalNode::Skip {
            name: relative.to_string(),
        });
        return Ok(());
    }
    if meta.is_dir() {
        nodes.push(LocalNode::Directory {
            relative: relative.to_string(),
        });
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) => {
                nodes.push(LocalNode::Failed {
                    name: relative.to_string(),
                    code: local_code(&error),
                });
                return Ok(());
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    nodes.push(LocalNode::Failed {
                        name: relative.to_string(),
                        code: local_code(&error),
                    });
                    continue;
                }
            };
            let child = match component(&entry.file_name()) {
                Ok(name) => format!("{relative}/{name}"),
                Err(name) => {
                    nodes.push(LocalNode::Skip { name });
                    continue;
                }
            };
            walk(&entry.path(), &child, depth + 1, max_files, files, nodes)?;
        }
        return Ok(());
    }
    if meta.is_file() {
        *files += 1;
        if *files > max_files {
            return Err(SshError::new("too_many"));
        }
        nodes.push(LocalNode::File {
            relative: relative.to_string(),
            source: path.to_path_buf(),
            bytes: meta.len(),
        });
        return Ok(());
    }
    nodes.push(LocalNode::Skip {
        name: relative.to_string(),
    });
    Ok(())
}

fn component(name: &std::ffi::OsStr) -> Result<String, String> {
    let text = name.to_string_lossy();
    if super::remote::valid_component(&text) {
        Ok(text.into_owned())
    } else {
        Err(text.into_owned())
    }
}

fn node_relative(node: &LocalNode) -> &str {
    match node {
        LocalNode::Directory { relative } | LocalNode::File { relative, .. } => relative,
        LocalNode::Skip { name } | LocalNode::Failed { name, .. } => name,
    }
}

fn leaf(relative: &str) -> &str {
    relative.rsplit('/').next().unwrap_or(relative)
}

fn blocked_code<'a>(blocked: &'a [(String, &'static str)], relative: &str) -> Option<&'static str> {
    blocked.iter().find_map(|(prefix, code)| {
        if relative == prefix || relative.starts_with(&format!("{prefix}/")) {
            Some(*code)
        } else {
            None
        }
    })
}

fn local_code(error: &std::io::Error) -> &'static str {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        "local_permission"
    } else {
        "transfer_failed"
    }
}

fn emit_file(
    progress: &Channel<Value>,
    task_id: u64,
    name: &str,
    status: &str,
    code: Option<&str>,
) {
    let mut value = json!({
        "type": "file",
        "taskId": task_id,
        "name": name,
        "status": status,
    });
    if let Some(code) = code {
        value["code"] = json!(code);
    }
    let _ = progress.send(value);
}

fn emit_progress(progress: &Channel<Value>, task_id: u64, name: &str, done: u64, total: u64) {
    let _ = progress.send(json!({
        "type": "progress",
        "taskId": task_id,
        "name": name,
        "done": done,
        "total": total,
    }));
}

fn is_missing(error: &russh_sftp::client::error::Error) -> bool {
    matches!(error, russh_sftp::client::error::Error::Status(status) if status.status_code == StatusCode::NoSuchFile)
}

fn map_sftp(error: &russh_sftp::client::error::Error) -> SshError {
    use russh_sftp::client::error::Error;
    match error {
        Error::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => SshError::new("remote_missing"),
            StatusCode::PermissionDenied => SshError::new("remote_permission"),
            StatusCode::NoConnection | StatusCode::ConnectionLost => SshError::new("disconnected"),
            _ if no_space(&status.error_message) => SshError::new("disk_full"),
            _ => SshError::new("transfer_failed"),
        },
        Error::Timeout => SshError::new("timeout"),
        Error::IO(text) if no_space(text) => SshError::new("disk_full"),
        Error::IO(_) => SshError::new("disconnected"),
        _ => SshError::new("transfer_failed"),
    }
}

fn map_local_io(error: &std::io::Error) -> SshError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        SshError::new("local_permission")
    } else {
        SshError::new("transfer_failed")
    }
}

fn map_remote_io(error: &std::io::Error) -> SshError {
    let text = error.to_string();
    let lower = text.to_ascii_lowercase();
    if error.kind() == std::io::ErrorKind::TimedOut || lower.contains("timed out") {
        return SshError::new("timeout");
    }
    if no_space(&text) {
        return SshError::new("disk_full");
    }
    if lower.contains("permission") {
        return SshError::new("remote_permission");
    }
    if lower.contains("no such") {
        return SshError::new("remote_missing");
    }
    if lower.contains("connection") {
        return SshError::new("disconnected");
    }
    SshError::new("transfer_failed")
}

fn no_space(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("no space") || lower.contains("disk full") || lower.contains("quota")
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<RemoteEntry>,
}

struct RemoteFile {
    relative: String,
    remote: String,
    bytes: u64,
}

pub async fn list_dir(sftp: &SftpSession, path: &str) -> Result<RemoteListing, SshError> {
    let mut entries = Vec::new();
    let listed = sftp
        .read_dir(path)
        .await
        .map_err(|error| map_sftp(&error))?;
    for entry in listed {
        let name = entry.file_name();
        if !super::remote::valid_component(&name) {
            continue;
        }
        let kind = match entry.file_type() {
            FileType::Dir => "dir",
            FileType::File => "file",
            FileType::Symlink => "link",
            FileType::Other => "other",
        };
        entries.push(RemoteEntry {
            name,
            path: entry.path(),
            kind: kind.to_string(),
            size: entry.metadata().size.unwrap_or(0),
        });
    }
    entries.sort_by(|left, right| {
        right
            .kind
            .eq("dir")
            .cmp(&left.kind.eq("dir"))
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
    });
    Ok(RemoteListing {
        path: path.to_string(),
        parent: parent_remote(path),
        entries,
    })
}

pub async fn make_dir(sftp: &SftpSession, path: &str) -> Result<(), SshError> {
    let path = checked_remote(path)?;
    sftp.create_dir(path)
        .await
        .map_err(|error| map_sftp(&error))
}

pub async fn rename_path(sftp: &SftpSession, from: &str, to: &str) -> Result<(), SshError> {
    let from = checked_remote(from)?;
    let to = checked_remote(to)?;
    if from == to {
        return Err(SshError::new("invalid_input"));
    }
    sftp.rename(from, to)
        .await
        .map_err(|error| map_sftp(&error))
}

pub async fn remove_path(sftp: &SftpSession, path: &str) -> Result<(), SshError> {
    let path = checked_remote(path)?;
    let mut pending = vec![(path, false)];
    let mut removed = 0usize;
    while let Some((current, expanded)) = pending.pop() {
        if removed > MAX_FILES || pending.len() > MAX_FILES {
            return Err(SshError::new("transfer_failed"));
        }
        if expanded {
            sftp.remove_dir(current)
                .await
                .map_err(|error| map_sftp(&error))?;
            removed += 1;
            continue;
        }
        let meta = sftp
            .symlink_metadata(&current)
            .await
            .map_err(|error| map_sftp(&error))?;
        if meta.file_type() != FileType::Dir {
            sftp.remove_file(current)
                .await
                .map_err(|error| map_sftp(&error))?;
            removed += 1;
            continue;
        }
        pending.push((current.clone(), true));
        let listed = sftp
            .read_dir(&current)
            .await
            .map_err(|error| map_sftp(&error))?;
        for entry in listed {
            let name = entry.file_name();
            if !valid_component(&name) {
                continue;
            }
            let child =
                join_remote(&current, &name).ok_or_else(|| SshError::new("invalid_input"))?;
            if entry.file_type() == FileType::Dir {
                pending.push((child, false));
            } else {
                sftp.remove_file(child)
                    .await
                    .map_err(|error| map_sftp(&error))?;
                removed += 1;
            }
        }
    }
    Ok(())
}

fn checked_remote(path: &str) -> Result<String, SshError> {
    let path = normalize_absolute(path).ok_or_else(|| SshError::new("invalid_input"))?;
    let name = path.rsplit('/').next().unwrap_or("");
    if path == "/" || !valid_component(name) {
        return Err(SshError::new("invalid_input"));
    }
    Ok(path)
}

pub async fn receive_all(
    sftp: &SftpSession,
    root: &Path,
    paths: &[String],
    task_id: u64,
    progress: &Channel<Value>,
    cancel: &mut watch::Receiver<bool>,
    gate: &DecisionGate,
) -> Result<(), SshError> {
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut skipped = 0u64;
    for path in paths {
        collect_remote(
            sftp,
            path,
            "",
            &mut files,
            &mut directories,
            &mut skipped,
            0,
        )
        .await?;
    }
    for directory in &directories {
        let path = local_within(root, directory).ok_or_else(|| SshError::new("invalid_input"))?;
        std::fs::create_dir_all(&path).map_err(|error| map_local_io(&error))?;
        if !stays_under(root, &path)? {
            return Err(SshError::new("transfer_failed"));
        }
    }
    let total_bytes = files
        .iter()
        .fold(0u64, |sum, file| sum.saturating_add(file.bytes));
    let _ = progress.send(json!({
        "type": "started",
        "taskId": task_id,
        "direction": "download",
        "target": root.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
        "files": files.len(),
        "total": total_bytes,
    }));
    let mut sent = 0u64;
    let mut downloaded = 0u64;
    let mut failed = 0u64;
    for file in &files {
        if *cancel.borrow() {
            return Err(SshError::new("cancelled"));
        }
        let name = file.relative.rsplit('/').next().unwrap_or(&file.relative);
        match save_remote_file(
            sftp,
            root,
            file,
            task_id,
            progress,
            cancel,
            gate,
            &mut sent,
            total_bytes,
        )
        .await
        {
            Ok(Item::Skipped) => skipped += 1,
            Ok(_) => downloaded += 1,
            Err(error) if error.code == "cancelled" => return Err(error),
            Err(error) => {
                failed += 1;
                emit_file(progress, task_id, name, "failed", Some(error.code));
            }
        }
    }
    let _ = progress.send(json!({
        "type": "finished",
        "taskId": task_id,
        "direction": "download",
        "uploaded": downloaded,
        "skipped": skipped,
        "failed": failed,
        "target": root.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
    }));
    Ok(())
}

async fn collect_remote(
    sftp: &SftpSession,
    remote: &str,
    parent: &str,
    files: &mut Vec<RemoteFile>,
    directories: &mut Vec<String>,
    skipped: &mut u64,
    depth: usize,
) -> Result<(), SshError> {
    let mut pending = vec![(remote.to_string(), parent.to_string(), depth)];
    while let Some((remote, parent, depth)) = pending.pop() {
        if files.len() > MAX_FILES || depth > 64 {
            return Err(SshError::new("too_many"));
        }
        let meta = sftp
            .symlink_metadata(&remote)
            .await
            .map_err(|error| map_sftp(&error))?;
        let name = remote
            .rsplit('/')
            .next()
            .filter(|part| !part.is_empty())
            .unwrap_or(remote.as_str());
        if !super::remote::valid_component(name) && remote != "/" {
            return Err(SshError::new("invalid_input"));
        }
        let relative = if parent.is_empty() {
            name.to_string()
        } else {
            format!("{parent}/{name}")
        };
        match meta.file_type() {
            FileType::Symlink | FileType::Other => *skipped += 1,
            FileType::File => files.push(RemoteFile {
                relative,
                remote,
                bytes: meta.size.unwrap_or(0),
            }),
            FileType::Dir => {
                directories.push(relative.clone());
                let listed = sftp
                    .read_dir(&remote)
                    .await
                    .map_err(|error| map_sftp(&error))?;
                for entry in listed {
                    let child = entry.file_name();
                    if !super::remote::valid_component(&child) {
                        continue;
                    }
                    pending.push((entry.path(), relative.clone(), depth + 1));
                }
            }
        }
    }
    Ok(())
}

async fn save_remote_file(
    sftp: &SftpSession,
    root: &Path,
    file: &RemoteFile,
    task_id: u64,
    progress: &Channel<Value>,
    cancel: &mut watch::Receiver<bool>,
    gate: &DecisionGate,
    sent: &mut u64,
    total_bytes: u64,
) -> Result<Item, SshError> {
    let mut destination =
        local_within(root, &file.relative).ok_or_else(|| SshError::new("invalid_input"))?;
    let name = file
        .relative
        .rsplit('/')
        .next()
        .unwrap_or(&file.relative)
        .to_string();
    if local_exists(&destination) {
        match gate.choose_for(progress, task_id, &name, cancel).await? {
            Policy::Skip => {
                *sent = sent.saturating_add(file.bytes);
                emit_progress(progress, task_id, &name, *sent, total_bytes);
                emit_file(progress, task_id, &name, "skipped", None);
                return Ok(Item::Skipped);
            }
            Policy::Rename => {
                destination =
                    local_renamed(&destination).ok_or_else(|| SshError::new("transfer_failed"))?;
            }
            Policy::Overwrite => {
                let meta = std::fs::symlink_metadata(&destination)
                    .map_err(|error| map_local_io(&error))?;
                if meta.is_dir() {
                    return Err(SshError::new("transfer_failed"));
                }
            }
        }
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| map_local_io(&error))?;
        if !stays_under(root, parent)? {
            return Err(SshError::new("transfer_failed"));
        }
    }
    let mut temp_name = destination.as_os_str().to_os_string();
    temp_name.push(".pixo-part");
    let temp = PathBuf::from(temp_name);
    let result = write_local_part(
        sftp,
        file,
        &temp,
        &name,
        task_id,
        progress,
        cancel,
        sent,
        total_bytes,
    )
    .await;
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
        return result.map(|_| Item::Uploaded);
    }
    if local_exists(&destination) {
        let meta = std::fs::symlink_metadata(&destination).map_err(|error| map_local_io(&error))?;
        if meta.is_dir() {
            let _ = std::fs::remove_file(&temp);
            return Err(SshError::new("transfer_failed"));
        }
        std::fs::remove_file(&destination).map_err(|error| map_local_io(&error))?;
    }
    if std::fs::rename(&temp, &destination).is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err(SshError::new("transfer_failed"));
    }
    emit_file(progress, task_id, &name, "done", None);
    Ok(Item::Uploaded)
}

async fn write_local_part(
    sftp: &SftpSession,
    file: &RemoteFile,
    temp: &Path,
    name: &str,
    task_id: u64,
    progress: &Channel<Value>,
    cancel: &watch::Receiver<bool>,
    sent: &mut u64,
    total_bytes: u64,
) -> Result<(), SshError> {
    let mut remote = sftp
        .open(&file.remote)
        .await
        .map_err(|error| map_sftp(&error))?;
    let mut local = tokio::fs::File::create(temp)
        .await
        .map_err(|error| map_local_io(&error))?;
    let mut buffer = vec![0u8; CHUNK];
    let mut reported = Instant::now() - Duration::from_secs(1);
    emit_progress(progress, task_id, name, *sent, total_bytes);
    loop {
        if *cancel.borrow() {
            return Err(SshError::new("cancelled"));
        }
        let read = remote
            .read(&mut buffer)
            .await
            .map_err(|error| map_remote_io(&error))?;
        if read == 0 {
            break;
        }
        local
            .write_all(&buffer[..read])
            .await
            .map_err(|error| map_local_io(&error))?;
        *sent = sent.saturating_add(read as u64);
        if reported.elapsed() >= Duration::from_millis(200) {
            emit_progress(progress, task_id, name, *sent, total_bytes);
            reported = Instant::now();
        }
    }
    local.flush().await.map_err(|error| map_local_io(&error))?;
    emit_progress(progress, task_id, name, *sent, total_bytes);
    Ok(())
}

pub fn local_within(root: &Path, relative: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    if relative.is_empty() {
        return Some(path);
    }
    for part in relative.split('/') {
        if !super::remote::valid_component(part) {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

fn local_exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn local_renamed(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let parent = path.parent()?;
    for index in 1..=999 {
        let next = renamed_file(name, index)?;
        let candidate = parent.join(&next);
        if !local_exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn stays_under(root: &Path, path: &Path) -> Result<bool, SshError> {
    let mut cursor = root.to_path_buf();
    if std::fs::symlink_metadata(&cursor)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(true)
    {
        return Ok(false);
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| SshError::new("transfer_failed"))?;
    for part in relative.components() {
        let std::path::Component::Normal(part) = part else {
            return Ok(false);
        };
        cursor.push(part);
        let meta = match std::fs::symlink_metadata(&cursor) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(map_local_io(&error)),
        };
        if meta.file_type().is_symlink() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

trait NamedError {
    fn or_name(self, _name: &str) -> Self;
}

impl NamedError for SshError {
    fn or_name(self, _name: &str) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::collect_limited;

    #[test]
    fn collects_a_directory_without_following_links() {
        let dir =
            std::env::temp_dir().join(format!("pixo-ssh-walk-{}", super::super::grant::nonce()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested/a.txt"), b"abc").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("a.txt", dir.join("nested/link")).unwrap();
        let nodes = collect_limited(std::slice::from_ref(&dir), 10).unwrap();
        let names = nodes
            .iter()
            .map(|node| match node {
                super::LocalNode::Directory { relative } => format!("dir:{relative}"),
                super::LocalNode::File {
                    relative, bytes, ..
                } => format!("file:{relative}:{bytes}"),
                super::LocalNode::Skip { name } => format!("skip:{name}"),
                super::LocalNode::Failed { name, code } => format!("fail:{name}:{code}"),
            })
            .collect::<Vec<_>>();
        let root = dir.file_name().unwrap().to_string_lossy();
        assert!(names.iter().any(|name| name == &format!("dir:{root}")));
        assert!(
            names
                .iter()
                .any(|name| name == &format!("file:{root}/nested/a.txt:3"))
        );
        #[cfg(unix)]
        assert!(
            names
                .iter()
                .any(|name| name == &format!("skip:{root}/nested/link"))
        );
        assert!(collect_limited(std::slice::from_ref(&dir), 0).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn download_paths_stay_inside_the_chosen_directory() {
        let root = std::path::Path::new("/tmp/pixo-download");
        let kept = super::local_within(root, "logs/app.log").unwrap();
        assert_eq!(kept, root.join("logs").join("app.log"));
        assert!(super::local_within(root, "../secret").is_none());
        assert!(super::local_within(root, "logs/../secret").is_none());
        assert!(super::local_within(root, "logs/").is_none());
    }
}
