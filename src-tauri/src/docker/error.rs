use serde::Serialize;

#[derive(Debug, Clone)]
pub struct DockerError {
    pub code: &'static str,
    pub params: serde_json::Value,
}

impl DockerError {
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            params: serde_json::json!({}),
        }
    }

    pub fn with_params(code: &'static str, params: serde_json::Value) -> Self {
        Self { code, params }
    }
}

impl std::fmt::Display for DockerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.code)
    }
}

impl std::error::Error for DockerError {}

impl Serialize for DockerError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("DockerError", 2)?;
        state.serialize_field("code", self.code)?;
        state.serialize_field("params", &self.params)?;
        state.end()
    }
}

pub fn log_docker(stage: &str, code: &str) {
    eprintln!("docker stage={stage} code={code}");
}

impl From<crate::ssh::SshError> for DockerError {
    fn from(error: crate::ssh::SshError) -> Self {
        Self {
            code: error.code,
            params: error.params,
        }
    }
}
