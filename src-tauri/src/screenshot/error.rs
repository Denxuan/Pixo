use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ScreenshotError {
    pub code: &'static str,
    pub params: serde_json::Value,
}

impl ScreenshotError {
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

impl std::fmt::Display for ScreenshotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.code)
    }
}

impl std::error::Error for ScreenshotError {}

impl Serialize for ScreenshotError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ScreenshotError", 2)?;
        state.serialize_field("code", self.code)?;
        state.serialize_field("params", &self.params)?;
        state.end()
    }
}
