use std::net::Ipv4Addr;
use std::sync::Arc;

use russh::client;
use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::TcpStream;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::watch;
use tokio::time::{Duration, timeout};

use super::dial::ClientHandler;

const HANDSHAKE: Duration = Duration::from_secs(5);

pub async fn serve(
    listener: tokio::net::TcpListener,
    handle: Arc<AsyncMutex<Option<client::Handle<ClientHandler>>>>,
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
                let handle = Arc::clone(&handle);
                let mut stop = cancel.clone();
                tauri::async_runtime::spawn(async move {
                    let mut socket = socket;
                    let target = tokio::select! {
                        _ = stop.changed() => None,
                        result = timeout(HANDSHAKE, handshake(&mut socket)) => result.ok().flatten(),
                    };
                    let Some((host, port)) = target else {
                        return;
                    };
                    let channel = {
                        let slot = handle.lock().await;
                        let Some(session) = slot.as_ref() else {
                            return;
                        };
                        session
                            .channel_open_direct_tcpip(host, port, "127.0.0.1", 0)
                            .await
                            .ok()
                    };
                    let Some(channel) = channel else {
                        return;
                    };
                    let mut remote = channel.into_stream();
                    tokio::select! {
                        _ = stop.changed() => {}
                        _ = copy_bidirectional(&mut socket, &mut remote) => {}
                    }
                });
            }
        }
    }
}

async fn handshake(socket: &mut TcpStream) -> Option<(String, u32)> {
    let mut head = [0u8; 2];
    socket.read_exact(&mut head).await.ok()?;
    if head[0] != 5 || head[1] == 0 || head[1] > 8 {
        return None;
    }
    let mut methods = vec![0u8; head[1] as usize];
    socket.read_exact(&mut methods).await.ok()?;
    socket.write_all(&[5, 0]).await.ok()?;
    let mut request = [0u8; 4];
    socket.read_exact(&mut request).await.ok()?;
    if request[0] != 5 || request[1] != 1 {
        let _ = socket.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        return None;
    }
    let host = match request[3] {
        1 => {
            let mut bytes = [0u8; 4];
            socket.read_exact(&mut bytes).await.ok()?;
            Ipv4Addr::from(bytes).to_string()
        }
        3 => {
            let mut len = [0u8; 1];
            socket.read_exact(&mut len).await.ok()?;
            if len[0] == 0 {
                return None;
            }
            let mut name = vec![0u8; len[0] as usize];
            socket.read_exact(&mut name).await.ok()?;
            let host = String::from_utf8(name).ok()?;
            if host.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
                return None;
            }
            host
        }
        4 => {
            let mut bytes = [0u8; 16];
            socket.read_exact(&mut bytes).await.ok()?;
            let host = std::net::Ipv6Addr::from(bytes).to_string();
            host
        }
        _ => return None,
    };
    let mut port_bytes = [0u8; 2];
    socket.read_exact(&mut port_bytes).await.ok()?;
    let port = u16::from_be_bytes(port_bytes);
    if port == 0 {
        return None;
    }
    socket
        .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
        .ok()?;
    Some((host, u32::from(port)))
}
