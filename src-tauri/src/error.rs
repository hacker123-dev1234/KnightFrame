use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type KfResult<T> = Result<T, LocalizedError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalizedError {
    pub key: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
}

impl LocalizedError {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            args: BTreeMap::new(),
        }
    }

    pub fn arg(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.args.insert(key.into(), value.to_string());
        self
    }
}

impl std::fmt::Display for LocalizedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.key)
    }
}

impl std::error::Error for LocalizedError {}

impl From<std::io::Error> for LocalizedError {
    fn from(error: std::io::Error) -> Self {
        Self::new("error.io").arg("detail", error)
    }
}
