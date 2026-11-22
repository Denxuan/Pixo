use russh::keys::agent::client::{AgentClient, AgentStream};

use super::error::SshError;

pub fn available() -> bool {
    #[cfg(unix)]
    {
        socket_path()
            .and_then(|path| std::os::unix::net::UnixStream::connect(path).ok())
            .is_some()
    }
    #[cfg(windows)]
    {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(r"\\.\pipe\openssh-ssh-agent")
            .is_ok()
    }
}

pub async fn open() -> Result<AgentClient<Box<dyn AgentStream + Send + Unpin>>, SshError> {
    #[cfg(unix)]
    {
        let Some(path) = socket_path() else {
            return Err(SshError::new("agent_unavailable"));
        };
        AgentClient::connect_uds(path)
            .await
            .map(|client| client.dynamic())
            .map_err(|_| SshError::new("agent_unavailable"))
    }
    #[cfg(windows)]
    {
        if let Ok(client) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
            return Ok(client.dynamic());
        }
        AgentClient::connect_pageant()
            .await
            .map(|client| client.dynamic())
            .map_err(|_| SshError::new("agent_unavailable"))
    }
}

#[cfg(unix)]
fn socket_path() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("SSH_AUTH_SOCK") {
        if !path.is_empty() {
            return Some(std::path::PathBuf::from(path));
        }
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("launchctl")
            .args(["getenv", "SSH_AUTH_SOCK"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8(output.stdout).ok()?;
        let path = path.trim();
        if path.is_empty() {
            return None;
        }
        return Some(std::path::PathBuf::from(path));
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub async fn relay(channel: russh::Channel<russh::client::Msg>) {
    match open_forward().await {
        Ok(local) => copy_agent(channel, local).await,
        Err(()) => {
            let _ = channel.close().await;
        }
    }
}

async fn copy_agent<L>(channel: russh::Channel<russh::client::Msg>, mut local: L)
where
    L: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut remote = channel.into_stream();
    let _ = tokio::io::copy_bidirectional(&mut remote, &mut local).await;
}

#[cfg(unix)]
async fn open_forward() -> Result<tokio::net::UnixStream, ()> {
    let path = socket_path().ok_or(())?;
    tokio::net::UnixStream::connect(path).await.map_err(|_| ())
}

#[cfg(windows)]
async fn open_forward() -> Result<tokio::net::windows::named_pipe::NamedPipeClient, ()> {
    tokio::net::windows::named_pipe::ClientOptions::new()
        .open(r"\\.\pipe\openssh-ssh-agent")
        .map_err(|_| ())
}
