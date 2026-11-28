use super::error::SshError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardKind {
    Local,
    Remote,
    Dynamic,
}

impl ForwardKind {
    pub fn parse(value: &str) -> Result<Self, SshError> {
        match value {
            "local" => Ok(Self::Local),
            "remote" => Ok(Self::Remote),
            "dynamic" => Ok(Self::Dynamic),
            _ => Err(SshError::new("invalid_input")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::Dynamic => "dynamic",
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardTarget {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardSpec {
    pub kind: ForwardKind,
    pub listen_port: u16,
    pub target_host: String,
    pub target_port: u16,
}

#[cfg(test)]
pub fn parse_forward(
    local_port: u32,
    remote_host: &str,
    remote_port: u32,
) -> Result<ForwardTarget, SshError> {
    let spec = parse_spec("local", local_port, remote_host, remote_port)?;
    Ok(ForwardTarget {
        local_port: spec.listen_port,
        remote_host: spec.target_host,
        remote_port: spec.target_port,
    })
}

pub fn parse_spec(
    kind: &str,
    listen_port: u32,
    target_host: &str,
    target_port: u32,
) -> Result<ForwardSpec, SshError> {
    let kind = ForwardKind::parse(kind)?;
    if !(1..=65_535).contains(&listen_port) {
        return Err(SshError::new("invalid_input"));
    }
    if kind == ForwardKind::Dynamic {
        return Ok(ForwardSpec {
            kind,
            listen_port: listen_port as u16,
            target_host: String::new(),
            target_port: 0,
        });
    }
    if !(1..=65_535).contains(&target_port) || !valid_host(target_host.trim()) {
        return Err(SshError::new("invalid_input"));
    }
    Ok(ForwardSpec {
        kind,
        listen_port: listen_port as u16,
        target_host: target_host.trim().to_string(),
        target_port: target_port as u16,
    })
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | ':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_local_forward_and_rejects_a_blank_host() {
        let target = parse_forward(8080, "127.0.0.1", 80).unwrap();
        assert_eq!(target.local_port, 8080);
        assert_eq!(target.remote_port, 80);
        assert!(parse_forward(0, "127.0.0.1", 80).is_err());
        assert!(parse_forward(80, "bad host", 80).is_err());
    }

    #[test]
    fn dynamic_forward_ignores_the_target_and_remote_needs_one() {
        let dynamic = parse_spec("dynamic", 1080, "", 0).unwrap();
        assert_eq!(dynamic.kind, ForwardKind::Dynamic);
        assert_eq!(dynamic.listen_port, 1080);
        assert!(parse_spec("remote", 9000, "127.0.0.1", 3000).is_ok());
        assert!(parse_spec("remote", 9000, "", 3000).is_err());
        assert!(parse_spec("other", 80, "127.0.0.1", 80).is_err());
    }
}
