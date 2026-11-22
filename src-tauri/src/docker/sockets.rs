use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocketCandidate {
    pub path: String,
    pub hint: String,
}

pub fn socket_candidates(home: &Path, runtime: Option<&Path>, exists: impl Fn(&Path) -> bool) -> Vec<SocketCandidate> {
    let mut found = Vec::new();
    for (path, hint) in candidate_paths(home, runtime) {
        if exists(&path) && !found.iter().any(|item: &SocketCandidate| item.path == path.to_string_lossy()) {
            found.push(SocketCandidate {
                path: path.to_string_lossy().into_owned(),
                hint: hint.to_string(),
            });
        }
    }
    found
}

fn candidate_paths(home: &Path, runtime: Option<&Path>) -> Vec<(PathBuf, &'static str)> {
    let mut paths = Vec::new();
    paths.push((home.join(".docker").join("run").join("docker.sock"), "docker-desktop"));
    paths.push((home.join(".orbstack").join("run").join("docker.sock"), "orbstack"));
    paths.push((home.join(".colima").join("default").join("docker.sock"), "colima"));
    paths.push((home.join(".rd").join("docker.sock"), "rancher"));
    paths.push((PathBuf::from("/var/run/docker.sock"), "engine"));
    if let Some(runtime) = runtime {
        paths.push((runtime.join("docker.sock"), "docker-rootless"));
        paths.push((runtime.join("podman").join("podman.sock"), "podman"));
    }
    paths.push((PathBuf::from("/run/podman/podman.sock"), "podman"));
    paths.push((
        home.join(".local")
            .join("share")
            .join("containers")
            .join("podman")
            .join("machine")
            .join("podman.sock"),
        "podman",
    ));
    paths.push((PathBuf::from(r"\\.\pipe\docker_engine"), "docker-desktop"));
    paths.push((PathBuf::from(r"\\.\pipe\podman-machine-default"), "podman"));
    paths
}

pub fn discover_sockets() -> Vec<SocketCandidate> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    socket_candidates(&home, runtime.as_deref(), path_exists)
}

fn path_exists(path: &Path) -> bool {
    #[cfg(unix)]
    {
        path.exists()
    }
    #[cfg(windows)]
    {
        named_pipe_exists(path)
    }
}

#[cfg(windows)]
fn named_pipe_exists(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Pipes::WaitNamedPipeW;
    use windows::core::PCWSTR;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    unsafe { WaitNamedPipeW(PCWSTR(wide.as_ptr()), 1).is_ok() }
}

pub fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::socket_candidates;
    use std::path::Path;

    #[test]
    fn only_existing_sockets_are_returned_in_probe_order() {
        let home = Path::new("/Users/dev");
        let found = socket_candidates(home, Some(Path::new("/run/user/501")), |path| {
            path.ends_with("docker.sock")
                && (path.starts_with("/Users/dev/.docker") || path.starts_with("/var/run"))
        });
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].hint, "docker-desktop");
        assert_eq!(found[1].hint, "engine");
    }
}
