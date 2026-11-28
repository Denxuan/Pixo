use std::path::{Component, Path, PathBuf};

use super::error::DockerError;
use super::model::{HostDirEntry, HostDirListing};

const SCAN_LIMIT: usize = 4000;
const SHOW_LIMIT: usize = 500;

pub fn list_local(raw: &str) -> Result<HostDirListing, DockerError> {
    let path = start_local(raw)?;
    let meta = std::fs::metadata(&path).map_err(map_io)?;
    if !meta.is_dir() {
        return Err(DockerError::new("dir_missing"));
    }
    let read = std::fs::read_dir(&path).map_err(map_io)?;
    let mut entries = Vec::new();
    let mut seen = 0;
    for item in read {
        seen += 1;
        if seen > SCAN_LIMIT {
            break;
        }
        let Ok(item) = item else {
            continue;
        };
        let name = item.file_name();
        let name = name.to_string_lossy();
        if name.is_empty() || name.contains('\0') || name == "." || name == ".." {
            continue;
        }
        let is_dir = item.metadata().map(|meta| meta.is_dir()).unwrap_or(false);
        if !is_dir {
            continue;
        }
        entries.push(HostDirEntry {
            name: name.into_owned(),
            path: item.path().to_string_lossy().into_owned(),
        });
    }
    entries.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    entries.truncate(SHOW_LIMIT);
    Ok(HostDirListing {
        path: path.to_string_lossy().into_owned(),
        parent: parent_local(&path),
        entries,
    })
}

fn start_local(raw: &str) -> Result<PathBuf, DockerError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(default_root());
    }
    if raw.len() > 1024 || raw.contains(['\0', '\n', '\r']) {
        return Err(DockerError::new("invalid_input"));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(DockerError::new("invalid_input"));
    }
    Ok(path)
}

fn default_root() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"C:\")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/")
    }
}

fn parent_local(path: &Path) -> Option<String> {
    let parent = path.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    Some(parent.to_string_lossy().into_owned())
}

fn map_io(error: std::io::Error) -> DockerError {
    match error.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory => {
            DockerError::new("dir_missing")
        }
        std::io::ErrorKind::PermissionDenied => DockerError::new("dir_denied"),
        _ => DockerError::new("unreachable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn lists_child_directories_and_keeps_their_parent() {
        let root = std::env::temp_dir().join(format!(
            "pixo-hostdir-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("beta")).expect("dir");
        fs::create_dir_all(root.join("Alpha")).expect("dir");
        fs::write(root.join("note.txt"), b"x").expect("file");
        let listing = list_local(&root.to_string_lossy()).expect("list");
        let names: Vec<_> = listing
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, vec!["Alpha", "beta"]);
        assert_eq!(
            Path::new(listing.parent.as_deref().expect("parent")),
            root.parent().expect("temp parent")
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_relative_parent_and_missing_paths() {
        assert_eq!(
            list_local("relative").expect_err("relative").code,
            "invalid_input"
        );
        assert_eq!(
            list_local("/tmp/../etc").expect_err("parent").code,
            "invalid_input"
        );
        #[cfg(not(windows))]
        assert_eq!(
            list_local("/no/such/pixo-folder")
                .expect_err("missing")
                .code,
            "dir_missing"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn empty_path_lists_the_filesystem_root() {
        let listing = list_local("").expect("root");
        assert_eq!(listing.path, "/");
        assert!(listing.parent.is_none());
    }
}
