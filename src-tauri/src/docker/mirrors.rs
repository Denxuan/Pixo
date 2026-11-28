use serde_json::{Map, Value};

use super::error::DockerError;

const STDOUT_LIMIT: usize = 256 * 1024;
const STDERR_LIMIT: usize = 8 * 1024;
const PAYLOAD: &str = "__PIXO_MIRROR_PAYLOAD__";
const PROFILE: &str = "__PIXO_PROFILE__";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonTarget {
    System,
    Rootless,
    Desktop,
    Colima,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonPlan {
    pub target: DaemonTarget,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
}

pub fn normalize_mirrors(input: &[String]) -> Result<Vec<String>, DockerError> {
    if input.len() > 8 {
        return Err(DockerError::new("invalid_input"));
    }
    let mut mirrors = Vec::new();
    for raw in input {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > 200 {
            return Err(DockerError::new("invalid_input"));
        }
        let value = trimmed.trim_end_matches('/').to_string();
        if !valid_mirror(&value) {
            return Err(DockerError::new("invalid_input"));
        }
        if mirrors
            .iter()
            .any(|item: &String| item.eq_ignore_ascii_case(&value))
        {
            return Err(DockerError::new("invalid_input"));
        }
        mirrors.push(value);
    }
    Ok(mirrors)
}

pub fn merge_daemon(existing: &str, mirrors: &[String]) -> Result<String, DockerError> {
    let trimmed = existing.trim().trim_start_matches('\u{feff}');
    let mut value: Value = if trimmed.is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(trimmed).map_err(|_| DockerError::new("mirror_config"))?
    };
    let Some(object) = value.as_object_mut() else {
        return Err(DockerError::new("mirror_config"));
    };
    if mirrors.is_empty() {
        object.remove("registry-mirrors");
    } else {
        object.insert(
            "registry-mirrors".to_string(),
            Value::Array(mirrors.iter().cloned().map(Value::String).collect()),
        );
    }
    let mut text =
        serde_json::to_string_pretty(&value).map_err(|_| DockerError::new("mirror_config"))?;
    text.push('\n');
    Ok(text)
}

pub fn encode_base64(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut index = 0;
    while index + 3 <= data.len() {
        let chunk =
            ((data[index] as u32) << 16) | ((data[index + 1] as u32) << 8) | data[index + 2] as u32;
        out.push(TABLE[((chunk >> 18) & 63) as usize] as char);
        out.push(TABLE[((chunk >> 12) & 63) as usize] as char);
        out.push(TABLE[((chunk >> 6) & 63) as usize] as char);
        out.push(TABLE[(chunk & 63) as usize] as char);
        index += 3;
    }
    match data.len() - index {
        1 => {
            let chunk = (data[index] as u32) << 16;
            out.push(TABLE[((chunk >> 18) & 63) as usize] as char);
            out.push(TABLE[((chunk >> 12) & 63) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let chunk = ((data[index] as u32) << 16) | ((data[index + 1] as u32) << 8);
            out.push(TABLE[((chunk >> 18) & 63) as usize] as char);
            out.push(TABLE[((chunk >> 12) & 63) as usize] as char);
            out.push(TABLE[((chunk >> 6) & 63) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

pub fn daemon_plan(
    local: bool,
    host_macos: bool,
    os_type: &str,
    operating_system: &str,
    security: &[String],
    socket: &str,
) -> Result<DaemonPlan, DockerError> {
    let os_name = operating_system.to_ascii_lowercase();
    if os_type.eq_ignore_ascii_case("windows") || os_name.contains("windows") {
        return Err(DockerError::new("mirror_unsupported"));
    }
    if !os_type.is_empty() && !os_type.eq_ignore_ascii_case("linux") {
        return Err(DockerError::new("mirror_unsupported"));
    }
    let socket_name = socket.to_ascii_lowercase();
    if socket_name.contains("orbstack")
        || socket_name.contains("rancher")
        || socket_name.contains("/.rd/")
        || socket_name.contains(".lima/")
    {
        return Err(DockerError::new("mirror_unsupported"));
    }
    if rootless(security) {
        return Ok(plan(DaemonTarget::Rootless, "default"));
    }
    if socket_name.contains(".colima/") || socket_name.contains(".colima\\") {
        return Ok(plan(DaemonTarget::Colima, &colima_profile(socket)));
    }
    // A Mac talks to Docker Desktop through a local socket. The mirrors live in
    // the user's ~/.docker/daemon.json, which the Desktop app loads on restart.
    if local && host_macos {
        return Ok(plan(DaemonTarget::Desktop, "default"));
    }
    Ok(plan(DaemonTarget::System, "default"))
}

pub fn read_script(plan: &DaemonPlan) -> Result<String, DockerError> {
    let profile = checked_profile(&plan.profile)?;
    let script = match plan.target {
        DaemonTarget::System => SYSTEM_READ,
        DaemonTarget::Rootless => ROOTLESS_READ,
        DaemonTarget::Desktop => DESKTOP_READ,
        DaemonTarget::Colima => COLIMA_READ,
    };
    Ok(script.replace(PROFILE, profile))
}

pub fn write_script(plan: &DaemonPlan, payload: &str) -> Result<String, DockerError> {
    if payload.is_empty()
        || payload.len() > 400_000
        || payload.contains(PAYLOAD)
        || !payload
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
    {
        return Err(DockerError::new("mirror_config"));
    }
    let profile = checked_profile(&plan.profile)?;
    let body = match plan.target {
        DaemonTarget::System => SYSTEM_APPLY,
        DaemonTarget::Rootless => ROOTLESS_APPLY,
        DaemonTarget::Desktop => DESKTOP_APPLY,
        DaemonTarget::Colima => COLIMA_APPLY,
    };
    Ok(format!("{DECODE}\n{body}")
        .replace(PAYLOAD, payload)
        .replace(PROFILE, profile))
}

pub fn capture(status: Option<i32>, stdout: &[u8], stderr: &[u8]) -> ShellOutput {
    let truncated = stdout.len() > STDOUT_LIMIT;
    let stderr_end = STDERR_LIMIT.min(stderr.len());
    ShellOutput {
        status,
        stdout: if truncated {
            String::new()
        } else {
            String::from_utf8_lossy(stdout).into_owned()
        },
        stderr: String::from_utf8_lossy(&stderr[..stderr_end]).into_owned(),
        truncated,
    }
}

pub fn command_stdout(output: &ShellOutput) -> Result<String, DockerError> {
    map_shell(output.status, &output.stderr)?;
    if output.truncated {
        return Err(DockerError::new("mirror_config"));
    }
    Ok(output.stdout.clone())
}

pub fn map_shell(status: Option<i32>, stderr: &str) -> Result<(), DockerError> {
    if status == Some(0) {
        return Ok(());
    }
    if status == Some(13) || stderr.contains("pixo-mirror-denied") {
        return Err(DockerError::new("mirror_permission"));
    }
    if status == Some(14) {
        return Err(DockerError::new("mirror_restart"));
    }
    Err(DockerError::new("mirror_apply"))
}

fn plan(target: DaemonTarget, profile: &str) -> DaemonPlan {
    DaemonPlan {
        target,
        profile: profile.to_string(),
    }
}

fn valid_mirror(value: &str) -> bool {
    if value.is_empty() || value.len() > 200 {
        return false;
    }
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return false;
    }
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"));
    let Some(rest) = rest else {
        return false;
    };
    if rest.is_empty()
        || rest.contains('@')
        || rest.contains('?')
        || rest.contains('#')
        || rest.contains('\\')
        || rest.contains("..")
    {
        return false;
    }
    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty()
        || host.starts_with('.')
        || host.ends_with('.')
        || host.starts_with('-')
        || host.ends_with('-')
        || !host.chars().any(|ch| ch.is_ascii_alphanumeric())
    {
        return false;
    }
    host.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | ':' | '[' | ']'))
        && rest[host.len()..]
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_' | '~' | ':'))
}

fn rootless(security: &[String]) -> bool {
    security.iter().any(|item| {
        item.split(',').any(|part| {
            let part = part.trim();
            part.eq_ignore_ascii_case("rootless") || part.eq_ignore_ascii_case("name=rootless")
        })
    })
}

fn colima_profile(socket: &str) -> String {
    socket
        .split(".colima/")
        .nth(1)
        .or_else(|| socket.split(".colima\\").nth(1))
        .and_then(|rest| rest.split(['/', '\\']).next())
        .filter(|profile| valid_profile(profile))
        .unwrap_or("default")
        .to_string()
}

fn valid_profile(profile: &str) -> bool {
    !profile.is_empty()
        && profile.len() <= 64
        && profile
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn checked_profile(profile: &str) -> Result<&str, DockerError> {
    if valid_profile(profile) {
        Ok(profile)
    } else {
        Err(DockerError::new("mirror_unsupported"))
    }
}

const SYSTEM_READ: &str = r#"set -eu
export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
if [ "$(id -u)" -eq 0 ]; then
  if [ -f /etc/docker/daemon.json ]; then
    cat /etc/docker/daemon.json
  fi
elif sudo -n true >/dev/null 2>&1; then
  if sudo -n test -f /etc/docker/daemon.json; then
    sudo -n cat /etc/docker/daemon.json
  fi
else
  echo pixo-mirror-denied >&2
  exit 13
fi
"#;

const ROOTLESS_READ: &str = r#"set -eu
export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
file="${XDG_CONFIG_HOME:-$HOME/.config}/docker/daemon.json"
if [ -f "$file" ]; then
  cat "$file"
fi
"#;

const DESKTOP_READ: &str = r#"set -eu
export PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:$PATH"
file="$HOME/.docker/daemon.json"
if [ -f "$file" ]; then
  cat "$file"
fi
"#;

const COLIMA_READ: &str = r#"set -eu
export PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:$PATH"
dir="$HOME/.colima/__PIXO_PROFILE__"
if [ ! -d "$dir" ]; then
  echo pixo-mirror-denied >&2
  exit 13
fi
if [ -f "$dir/daemon.json" ]; then
  cat "$dir/daemon.json"
fi
"#;

const DECODE: &str = r#"set -eu
export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
payload='__PIXO_MIRROR_PAYLOAD__'
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
if ! printf '%s' "$payload" | base64 -d > "$tmp" 2>/dev/null; then
  printf '%s' "$payload" | base64 -D > "$tmp"
fi
sig=$(head -c 1 "$tmp")
if [ "$sig" != "{" ]; then
  exit 1
fi
"#;

const SYSTEM_APPLY: &str = r#"dest=/etc/docker/daemon.json
if [ "$(id -u)" -eq 0 ]; then
  mkdir -p /etc/docker
  if [ -f "$dest" ]; then
    cp "$dest" "$dest.pixo.bak"
  fi
  cp "$tmp" "$dest.pixo.new"
  mv "$dest.pixo.new" "$dest"
  chmod 644 "$dest"
  if command -v systemctl >/dev/null 2>&1; then
    systemctl restart docker || exit 14
  elif command -v service >/dev/null 2>&1; then
    service docker restart || exit 14
  else
    exit 14
  fi
elif sudo -n true >/dev/null 2>&1; then
  sudo -n mkdir -p /etc/docker
  if sudo -n test -f "$dest"; then
    sudo -n cp "$dest" "$dest.pixo.bak"
  fi
  sudo -n cp "$tmp" "$dest.pixo.new"
  sudo -n mv "$dest.pixo.new" "$dest"
  sudo -n chmod 644 "$dest"
  if command -v systemctl >/dev/null 2>&1; then
    sudo -n systemctl restart docker || exit 14
  elif command -v service >/dev/null 2>&1; then
    sudo -n service docker restart || exit 14
  else
    exit 14
  fi
else
  echo pixo-mirror-denied >&2
  exit 13
fi
i=0
while [ "$i" -lt 15 ]; do
  if docker info >/dev/null 2>&1; then
    exit 0
  fi
  i=$((i + 1))
  sleep 1
done
exit 0
"#;

const ROOTLESS_APPLY: &str = r#"dir="${XDG_CONFIG_HOME:-$HOME/.config}/docker"
mkdir -p "$dir"
dest="$dir/daemon.json"
if [ -f "$dest" ]; then
  cp "$dest" "$dest.pixo.bak"
fi
cp "$tmp" "$dest.pixo.new"
mv "$dest.pixo.new" "$dest"
chmod 644 "$dest"
if command -v systemctl >/dev/null 2>&1; then
  systemctl --user restart docker || exit 14
else
  exit 14
fi
i=0
while [ "$i" -lt 15 ]; do
  if docker info >/dev/null 2>&1; then
    exit 0
  fi
  i=$((i + 1))
  sleep 1
done
exit 0
"#;

const DESKTOP_APPLY: &str = r#"export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
dir="$HOME/.docker"
mkdir -p "$dir"
dest="$dir/daemon.json"
if [ -f "$dest" ]; then
  cp "$dest" "$dest.pixo.bak"
fi
cp "$tmp" "$dest.pixo.new"
mv "$dest.pixo.new" "$dest"
chmod 644 "$dest"
if [ "$(uname)" != "Darwin" ]; then
  exit 14
fi
osascript -e 'quit app "Docker"' || exit 14
i=0
while [ "$i" -lt 30 ]; do
  if ! docker info >/dev/null 2>&1; then
    break
  fi
  i=$((i + 1))
  sleep 1
done
if docker info >/dev/null 2>&1; then
  exit 14
fi
open -a Docker || exit 14
i=0
while [ "$i" -lt 60 ]; do
  if docker info >/dev/null 2>&1; then
    exit 0
  fi
  i=$((i + 1))
  sleep 1
done
exit 14
"#;

const COLIMA_APPLY: &str = r#"export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
dir="$HOME/.colima/__PIXO_PROFILE__"
if [ ! -d "$dir" ]; then
  echo pixo-mirror-denied >&2
  exit 13
fi
dest="$dir/daemon.json"
if [ -f "$dest" ]; then
  cp "$dest" "$dest.pixo.bak"
fi
cp "$tmp" "$dest.pixo.new"
mv "$dest.pixo.new" "$dest"
chmod 644 "$dest"
if command -v colima >/dev/null 2>&1; then
  colima restart || exit 14
else
  exit 14
fi
i=0
while [ "$i" -lt 30 ]; do
  if docker info >/dev/null 2>&1; then
    exit 0
  fi
  i=$((i + 1))
  sleep 1
done
exit 0
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn mirrors(values: &[&str]) -> Vec<String> {
        values.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn mirrors_keep_https_hosts_and_drop_a_trailing_slash() {
        let saved = normalize_mirrors(&mirrors(&[
            " https://docker.example.com/ ",
            "http://127.0.0.1:5000/",
        ]))
        .unwrap();
        assert_eq!(
            saved,
            mirrors(&["https://docker.example.com", "http://127.0.0.1:5000"])
        );
    }

    #[test]
    fn mirrors_reject_credentials_duplicates_and_other_schemes() {
        assert!(normalize_mirrors(&mirrors(&["https://user:pass@example.com"])).is_err());
        assert!(normalize_mirrors(&mirrors(&["file:///var/lib/mirror"])).is_err());
        assert!(
            normalize_mirrors(&mirrors(&[
                "https://Mirror.Example.com",
                "https://mirror.example.com"
            ]))
            .is_err()
        );
        assert!(
            normalize_mirrors(&mirrors(&[
                "https://a.example",
                "https://b.example",
                "https://c.example",
                "https://d.example",
                "https://e.example",
                "https://f.example",
                "https://g.example",
                "https://h.example",
                "https://i.example"
            ]))
            .is_err()
        );
        assert!(normalize_mirrors(&[]).unwrap().is_empty());
    }

    #[test]
    fn merge_keeps_other_daemon_settings() {
        let merged = merge_daemon(
            r#"{"data-root":"/var/lib/docker","registry-mirrors":["https://old.example"]}"#,
            &mirrors(&["https://mirror.example.com"]),
        )
        .unwrap();
        assert_eq!(
            merged,
            "{\n  \"data-root\": \"/var/lib/docker\",\n  \"registry-mirrors\": [\n    \"https://mirror.example.com\"\n  ]\n}\n"
        );
        let cleared = merge_daemon(&merged, &[]).unwrap();
        assert_eq!(cleared, "{\n  \"data-root\": \"/var/lib/docker\"\n}\n");
        assert!(merge_daemon("[]", &[]).is_err());
        assert!(merge_daemon("{not json", &[]).is_err());
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn remote_linux_uses_the_system_daemon_file() {
        let plan = daemon_plan(
            false,
            true,
            "linux",
            "Ubuntu 24.04 LTS",
            &[],
            "/var/run/docker.sock",
        )
        .unwrap();
        assert_eq!(plan.target, DaemonTarget::System);
        let script = write_script(&plan, "aGVsbG8=").unwrap();
        assert!(script.contains("payload='aGVsbG8='"));
        assert!(script.contains("sudo -n systemctl restart docker"));
        assert!(script.contains("/etc/docker/daemon.json"));
        assert!(write_script(&plan, "abc'").is_err());
    }

    #[test]
    fn local_mac_desktop_rootless_and_colima_choose_their_own_files() {
        let desktop =
            daemon_plan(true, true, "linux", "Ubuntu", &[], "/var/run/docker.sock").unwrap();
        assert_eq!(desktop.target, DaemonTarget::Desktop);
        assert!(
            read_script(&desktop)
                .unwrap()
                .contains("$HOME/.docker/daemon.json")
        );

        let rootless = daemon_plan(
            false,
            false,
            "linux",
            "Ubuntu",
            &["name=rootless".into()],
            "/var/run/docker.sock",
        )
        .unwrap();
        assert_eq!(rootless.target, DaemonTarget::Rootless);

        let colima = daemon_plan(
            true,
            true,
            "linux",
            "Ubuntu",
            &[],
            "/Users/me/.colima/lab/docker.sock",
        )
        .unwrap();
        assert_eq!(colima.target, DaemonTarget::Colima);
        assert_eq!(colima.profile, "lab");
        assert!(read_script(&colima).unwrap().contains(".colima/lab"));
        assert!(!read_script(&colima).unwrap().contains("__PIXO_PROFILE__"));
    }

    #[test]
    fn windows_and_orbstack_stay_unsupported() {
        assert!(daemon_plan(true, false, "windows", "Windows", &[], "").is_err());
        assert!(
            daemon_plan(
                true,
                true,
                "linux",
                "Ubuntu",
                &[],
                "/Users/me/.orbstack/run/docker.sock"
            )
            .is_err()
        );
    }

    #[test]
    fn shell_status_maps_to_a_stable_error() {
        assert!(map_shell(Some(0), "").is_ok());
        assert_eq!(
            map_shell(Some(13), "").unwrap_err().code,
            "mirror_permission"
        );
        assert_eq!(
            map_shell(Some(2), "pixo-mirror-denied").unwrap_err().code,
            "mirror_permission"
        );
        assert_eq!(map_shell(Some(14), "").unwrap_err().code, "mirror_restart");
        assert_eq!(map_shell(None, "").unwrap_err().code, "mirror_apply");
        let huge = capture(Some(0), &vec![b'{'; STDOUT_LIMIT + 1], b"");
        assert!(command_stdout(&huge).is_err());
    }
}
