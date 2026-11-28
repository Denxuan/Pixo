use serde::Serialize;

pub const UPDATE: &str = "update";
pub const CURRENT: &str = "current";
pub const UNKNOWN: &str = "unknown";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageUpdate {
    pub reference: String,
    pub state: String,
}

pub struct ImageUse {
    pub name: String,
    pub image: String,
    pub image_id: String,
}

pub fn registry_state(
    reference: &str,
    repo_digests: &[String],
    remote_digest: Option<&str>,
) -> &'static str {
    let Some(remote) = remote_digest
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return UNKNOWN;
    };
    let (name, _) = split_ref(reference);
    let want = canonical_repo(name);
    let mut saw = false;
    for item in repo_digests {
        let Some((repo, digest)) = item.rsplit_once('@') else {
            continue;
        };
        if canonical_repo(repo) != want {
            continue;
        }
        saw = true;
        if digest_eq(digest, remote) {
            return CURRENT;
        }
    }
    if saw { UPDATE } else { UNKNOWN }
}

pub fn replaced_image_to_remove(
    previous_id: &str,
    current_id: Option<&str>,
    repo_tags: &[String],
    containers: &[ImageUse],
) -> Option<String> {
    let previous = previous_id.trim();
    let current = current_id
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    if previous.is_empty() || same_id(previous, current) || image_has_tag(repo_tags) {
        return None;
    }
    let used = containers.iter().any(|container| {
        same_id(&container.image_id, previous) || same_id(&container.image, previous)
    });
    if used {
        return None;
    }
    Some(previous.to_string())
}

fn image_has_tag(repo_tags: &[String]) -> bool {
    repo_tags.iter().any(|tag| {
        let tag = tag.trim();
        !tag.is_empty() && tag != "<none>:<none>"
    })
}

pub fn containers_using(reference: &str, image_id: &str, containers: &[ImageUse]) -> Vec<String> {
    let mut names = Vec::new();
    for container in containers {
        let name = container.name.trim().trim_start_matches('/');
        if name.is_empty() {
            continue;
        }
        let used = same_id(&container.image_id, image_id)
            || same_id(&container.image, image_id)
            || same_ref(&container.image, reference);
        if used && !names.iter().any(|item: &String| item == name) {
            names.push(name.to_string());
        }
        if names.len() == 8 {
            break;
        }
    }
    names
}

fn registry_state_is_known(reference: &str) -> bool {
    let (name, tag) = split_ref(reference);
    !name.is_empty() && !tag.is_empty() && !tag.contains('/')
}

pub fn checkable_reference(reference: &str) -> bool {
    registry_state_is_known(reference) && reference != "<none>:<none>"
}

fn digest_eq(left: &str, right: &str) -> bool {
    let left = left.trim().trim_start_matches("sha256:");
    let right = right.trim().trim_start_matches("sha256:");
    !left.is_empty() && left.eq_ignore_ascii_case(right)
}

fn same_id(left: &str, right: &str) -> bool {
    let left = normalize_id(left);
    let right = normalize_id(right);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if left == right {
        return true;
    }
    let (short, long) = if left.len() < right.len() {
        (left.as_str(), right.as_str())
    } else {
        (right.as_str(), left.as_str())
    };
    short.len() >= 12 && short.chars().all(|ch| ch.is_ascii_hexdigit()) && long.starts_with(short)
}

fn normalize_id(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("sha256:")
        .to_ascii_lowercase()
}

fn same_ref(left: &str, right: &str) -> bool {
    if left.trim().eq_ignore_ascii_case(right.trim()) {
        return true;
    }
    let (left_name, left_tag) = split_ref(left);
    let (right_name, right_tag) = split_ref(right);
    canonical_repo(left_name) == canonical_repo(right_name)
        && left_tag.eq_ignore_ascii_case(right_tag)
}

fn split_ref(value: &str) -> (&str, &str) {
    let value = value.trim();
    let value = value
        .rsplit_once('@')
        .map(|(name, _)| name)
        .unwrap_or(value);
    match value.rsplit_once(':') {
        Some((name, tag)) if !name.is_empty() && !tag.is_empty() && !tag.contains('/') => {
            (name, tag)
        }
        _ => (value, "latest"),
    }
}

fn canonical_repo(name: &str) -> String {
    let mut name = name.trim().trim_end_matches('/').to_ascii_lowercase();
    for prefix in ["https://", "http://"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest.to_string();
        }
    }
    for prefix in ["index.docker.io/", "registry-1.docker.io/", "docker.io/"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest.to_string();
        }
    }
    if !name.is_empty() && !name.contains('/') {
        name = format!("library/{name}");
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(name: &str, image: &str, image_id: &str) -> ImageUse {
        ImageUse {
            name: name.to_string(),
            image: image.to_string(),
            image_id: image_id.to_string(),
        }
    }

    #[test]
    fn registry_digest_marks_an_update_only_when_it_differs() {
        let digests = vec!["docker.io/library/postgres@sha256:aaa".to_string()];
        assert_eq!(
            registry_state("postgres:latest", &digests, Some("sha256:aaa")),
            CURRENT
        );
        assert_eq!(
            registry_state("postgres:latest", &digests, Some("sha256:bbb")),
            UPDATE
        );
        assert_eq!(registry_state("postgres:latest", &digests, None), UNKNOWN);
        assert_eq!(
            registry_state("postgres:16", &[], Some("sha256:aaa")),
            UNKNOWN
        );
        assert_eq!(
            registry_state(
                "localhost:5000/app:1",
                &["localhost:5000/app@sha256:abc".to_string()],
                Some("sha256:abc")
            ),
            CURRENT
        );
    }

    #[test]
    fn containers_using_the_tag_or_image_id_block_upgrade() {
        let containers = vec![
            user("db", "postgres:latest", "sha256:abc"),
            user("cache", "redis:latest", "sha256:defdefdefdef"),
        ];
        assert_eq!(
            containers_using("library/postgres:latest", "sha256:fff", &containers),
            vec!["db".to_string()]
        );
        assert_eq!(
            containers_using("postgres", "sha256:fff", &containers),
            vec!["db".to_string()]
        );
        assert_eq!(
            containers_using("redis:7", "sha256:defdefdefdef", &containers),
            vec!["cache".to_string()]
        );
        assert!(containers_using("mysql:8", "sha256:zzzzzzzzzzzz", &containers).is_empty());
    }

    #[test]
    fn upgrade_removes_only_an_untagged_unused_previous_image() {
        let previous = "sha256:818ae818ae818ae818ae818ae818ae818ae818ae818ae818ae818ae818ae818a";
        let current = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        assert_eq!(
            replaced_image_to_remove(previous, Some(current), &[], &[]).as_deref(),
            Some(previous)
        );
        assert_eq!(
            replaced_image_to_remove(previous, Some(current), &["<none>:<none>".to_string()], &[])
                .as_deref(),
            Some(previous)
        );
        assert!(replaced_image_to_remove(previous, Some(previous), &[], &[]).is_none());
        assert!(replaced_image_to_remove(previous, None, &[], &[]).is_none());
        assert!(
            replaced_image_to_remove(previous, Some(current), &["postgres:16".to_string()], &[])
                .is_none()
        );
        assert!(
            replaced_image_to_remove(
                previous,
                Some(current),
                &[],
                &[user("db", "postgres:16", previous)]
            )
            .is_none()
        );
    }
}
