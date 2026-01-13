use std::path::PathBuf;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct BeamRuntime {
    pub beam_url: reqwest::Url,
    pub beam_secret: String,
    pub beam_id: beam_lib::AppId,
}

#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Name of the receiving beam app without broker id e.g. app1.proxy2
    pub to: String,
    /// Name of the file to be read or '-' to read from stdin
    pub file: PathBuf,
    /// A suggestion for the new name the receiver should use. Will default to the uploaded files name if it is not read from stdin.
    name: Option<String>,
    /// Additional metadata for the file
    pub meta: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileMeta {
    #[serde(deserialize_with = "deserialize_filename")]
    pub suggested_name: Option<String>,

    pub meta: Option<serde_json::Value>,
}
impl Context {
    pub fn get_suggested_name(&self) -> Option<&str> {
        self.name
            .as_deref()
            .or_else(|| (self.file.as_os_str() != "-").then_some(self.file.file_name()?.to_str()?))
            .and_then(|v| validate_filename(v).ok())
    }
    pub fn to_file_meta(&self) -> FileMeta {
        FileMeta {
            suggested_name: self.get_suggested_name().map(ToOwned::to_owned),
            meta: self.meta.clone(),
        }
    }
}

pub fn validate_filename(name: &str) -> anyhow::Result<&str> {
    if name.chars().all(|c| c.is_alphanumeric() || ['_', '.', '-'].contains(&c)) {
        Ok(name)
    } else {
        Err(anyhow!("Invalid filename: {name}"))
    }
}

fn deserialize_filename<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let s = Option::<String>::deserialize(deserializer)?;
    if let Some(ref f) = s {
        validate_filename(f).map_err(serde::de::Error::custom)?;
    }
    Ok(s)
}
