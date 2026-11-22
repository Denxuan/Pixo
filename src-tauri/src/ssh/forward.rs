use super::error::SshError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardTarget {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

pub fn parse_forward(
    local_port: u32,
    remote_host: &str,
    remote_port: u32,
) -> Result<ForwardTarget, SshError> {
    if !(1..=65_535).contains(&local_port) || !(1..=65_535).contains(&remote_port) {
        return Err(SshError::new("invalid_input"));
    }
    let remote_host = remote_host.trim();
    if remote_host.is_empty()
        || remote_host.len() > 253
        || !remote_host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | ':'))
    {
        return Err(SshError::new("invalid_input"));
    }
    Ok(ForwardTarget {
        local_port: local_port as u16,
        remote_host: remote_host.to_string(),
        remote_port: remote_port as u16,
    })
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
}
