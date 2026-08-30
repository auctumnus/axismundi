use anyhow::{Result, bail};
use serde_json::Value;

pub(super) mod content;
pub(super) mod dictionary;
pub(super) mod structure;

#[derive(Debug)]
pub(super) struct ApiRequest {
    pub(super) method: &'static str,
    pub(super) path: String,
    pub(super) body: Option<Value>,
}

impl ApiRequest {
    pub(super) fn new(method: &'static str, path: String, body: Option<Value>) -> Self {
        Self { method, path, body }
    }
}

pub(super) fn path_segment<'a>(argument: &str, value: &'a str) -> Result<&'a str> {
    if value.is_empty()
        || value
            .chars()
            .any(|character| matches!(character, '/' | '?' | '#'))
    {
        bail!("{argument} must be a single non-empty path segment");
    }
    Ok(value)
}
