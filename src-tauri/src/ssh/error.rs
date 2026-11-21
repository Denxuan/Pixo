use serde::Serialize;

#[derive(Debug, Clone)]
pub struct SshError {
    pub code: &'static str,
    pub params: serde_json::Value,
}

impl SshError {
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

impl std::fmt::Display for SshError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.code)
    }
}

impl std::error::Error for SshError {}

impl Serialize for SshError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SshError", 2)?;
        state.serialize_field("code", self.code)?;
        state.serialize_field("params", &self.params)?;
        state.end()
    }
}

pub fn log_ssh(stage: &str, host_id: i64, code: &str) {
    eprintln!("ssh stage={stage} host_id={host_id} code={code}");
}
