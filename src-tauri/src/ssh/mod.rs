use tauri::Manager;

mod agent;
mod auth;
pub(crate) mod commands;
mod config_import;
mod cwd;
pub(crate) mod dial;
mod drops;
mod error;
mod flow;
mod forward;
mod grant;
mod known_hosts;
mod model;
mod remote;
mod secret;
mod session;
mod shell;
mod socks;
mod store;
mod transfer;

pub use drops::install as install_drop;
pub(crate) use error::SshError;
pub(crate) use session::SshState;
pub use session::initialize;

pub(crate) fn live_sessions(app: &tauri::AppHandle) -> usize {
    app.try_state::<session::SshState>()
        .map(|state| state.session_count())
        .unwrap_or(0)
}

pub(crate) fn hosts_json(app: &tauri::AppHandle) -> Result<String, String> {
    let state = app.try_state::<session::SshState>().ok_or("storage")?;
    let hosts = state.list().map_err(|error| error.code.to_string())?;
    serde_json::to_string_pretty(&hosts).map_err(|_| "storage".to_string())
}

pub(crate) fn menu_hosts(app: &tauri::AppHandle) -> (bool, Vec<(i64, String)>) {
    let Some(state) = app.try_state::<session::SshState>() else {
        return (false, Vec::new());
    };
    let enabled = state.status().map(|status| status.enabled).unwrap_or(false);
    if !enabled {
        return (false, Vec::new());
    }
    let hosts = state.list().unwrap_or_default();
    let items = hosts
        .into_iter()
        .take(9)
        .map(|host| {
            (
                host.id,
                menu_host_label(&host.name, &host.username, &host.host, host.port),
            )
        })
        .collect();
    (true, items)
}

pub(crate) fn menu_host_label(name: &str, username: &str, host: &str, port: u16) -> String {
    let host = host.trim();
    let address = if port == 22 {
        host.to_string()
    } else if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let name = name.trim();
    let username = username.trim();
    if name.is_empty() || name == host {
        if username.is_empty() {
            address
        } else {
            format!("{username}@{address}")
        }
    } else {
        // The tray keeps 48 characters. Shorten the name so the address stays visible.
        let gap = ' ';
        let room = 48usize.saturating_sub(address.chars().count().saturating_add(1));
        let mut short: String = name.chars().take(room).collect();
        if name.chars().count() > room && room > 1 {
            short.pop();
            short.push('…');
        }
        if short.is_empty() {
            address
        } else {
            format!("{short}{gap}{address}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::menu_host_label;

    #[test]
    fn menu_host_label_keeps_the_address_beside_the_name() {
        assert_eq!(
            menu_host_label("build", "dev", "10.0.0.8", 22),
            "build 10.0.0.8"
        );
        assert_eq!(
            menu_host_label("build", "dev", "10.0.0.8", 2222),
            "build 10.0.0.8:2222"
        );
        assert_eq!(menu_host_label("", "dev", "10.0.0.8", 22), "dev@10.0.0.8");
        assert_eq!(
            menu_host_label("10.0.0.8", "dev", "10.0.0.8", 22),
            "dev@10.0.0.8"
        );
        assert_eq!(
            menu_host_label("edge", "root", "2001:db8::1", 2222),
            "edge [2001:db8::1]:2222"
        );
        let long = "名".repeat(40);
        let label = menu_host_label(&long, "dev", "192.168.1.20", 22);
        assert!(label.ends_with(" 192.168.1.20"), "{label}");
        assert!(label.chars().count() <= 48, "{label}");
    }
}
