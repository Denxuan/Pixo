pub fn valid_component(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part.len() <= 255
        && !part.contains(['\0', '/', '\\'])
}

pub fn normalize_absolute(path: &str) -> Option<String> {
    if !path.starts_with('/') || path.contains('\0') {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            parts.pop();
            continue;
        }
        if !valid_component(part) {
            return None;
        }
        parts.push(part);
    }
    if parts.is_empty() {
        Some("/".to_string())
    } else {
        Some(format!("/{}", parts.join("/")))
    }
}

pub fn join_remote(dir: &str, relative: &str) -> Option<String> {
    let dir = normalize_absolute(dir)?;
    if relative.is_empty() {
        return Some(dir);
    }
    let mut parts = Vec::new();
    for part in relative.split('/') {
        if !valid_component(part) {
            return None;
        }
        parts.push(part);
    }
    if dir == "/" {
        Some(format!("/{}", parts.join("/")))
    } else {
        Some(format!("{dir}/{}", parts.join("/")))
    }
}

pub fn parent_remote(path: &str) -> Option<String> {
    let path = normalize_absolute(path)?;
    if path == "/" {
        return None;
    }
    match path.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(index) => Some(path[..index].to_string()),
        None => None,
    }
}

pub fn renamed_file(name: &str, index: u32) -> Option<String> {
    if !valid_component(name) || index == 0 || index > 999 {
        return None;
    }
    let next = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            format!("{stem} ({index}).{ext}")
        }
        _ => format!("{name} ({index})"),
    };
    if valid_component(&next) {
        Some(next)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{join_remote, normalize_absolute, parent_remote, renamed_file};

    #[test]
    fn normalizes_dot_segments() {
        assert_eq!(
            normalize_absolute("/opt/../zdx/./mysql").as_deref(),
            Some("/zdx/mysql")
        );
        assert_eq!(normalize_absolute("/../../etc").as_deref(), Some("/etc"));
        assert_eq!(normalize_absolute("/").as_deref(), Some("/"));
        assert!(normalize_absolute("opt").is_none());
        assert!(normalize_absolute("/opt\0").is_none());
    }

    #[test]
    fn joins_without_escaping_the_directory() {
        assert_eq!(
            join_remote("/opt/zdx", "mysql/a.txt").as_deref(),
            Some("/opt/zdx/mysql/a.txt")
        );
        assert_eq!(join_remote("/", "a.txt").as_deref(), Some("/a.txt"));
        assert!(join_remote("/opt", "../etc/passwd").is_none());
        assert!(join_remote("/opt", "a/../b").is_none());
        assert!(join_remote("/opt", "a\\b").is_none());
    }

    #[test]
    fn renames_before_the_last_extension() {
        assert_eq!(
            renamed_file("archive.tar.gz", 1).as_deref(),
            Some("archive.tar (1).gz")
        );
        assert_eq!(
            renamed_file(".gitignore", 2).as_deref(),
            Some(".gitignore (2)")
        );
        assert_eq!(parent_remote("/opt/zdx/a.txt").as_deref(), Some("/opt/zdx"));
        assert_eq!(parent_remote("/a.txt").as_deref(), Some("/"));
    }
}
