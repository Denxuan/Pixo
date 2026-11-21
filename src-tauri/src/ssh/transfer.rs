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
use super::remote::{join_remote, parent_remote, renamed_file};

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
}
