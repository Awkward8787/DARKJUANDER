use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config io: {0}")]
    Io(#[from] std::io::Error),
    #[error("config yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("config: {0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileConfig {
    pub socket: PathBuf,
    pub audit_path: PathBuf,
    pub policy_path: PathBuf,
    #[serde(default)]
    pub local_only: bool,
    #[serde(default)]
    pub prefer_cloud: bool,
    #[serde(default = "default_threshold")]
    pub psi_memory_some_avg10_threshold: f64,
    pub psi_memory_path: PathBuf,
    pub psi_io_path: PathBuf,
    #[serde(default)]
    pub allow_psi_override: bool,
    #[serde(default)]
    pub private_paths: Vec<PathBuf>,
    #[serde(default)]
    pub secret_paths: Vec<PathBuf>,
    pub models: BTreeMap<String, ModelConfig>,
    pub aliases: BTreeMap<String, AliasConfig>,
    pub engines: BTreeMap<String, EngineConfig>,
    pub principals: Vec<PrincipalConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelConfig {
    pub engine: String,
    pub location: String,
    pub size: String,
    pub memory_mb: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AliasConfig {
    pub small: String,
    pub mid: String,
    #[serde(default)]
    pub cloud: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineConfig {
    pub kind: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrincipalConfig {
    pub id: String,
    pub token: String,
    pub trust_domain: String,
    #[serde(default)]
    pub grants: Vec<String>,
    #[serde(default)]
    pub local_only: bool,
}

fn default_threshold() -> f64 {
    25.0
}

impl FileConfig {
    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path)?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let cfg: Self = serde_yaml::from_str(text)?;
        if cfg.models.is_empty() {
            return Err(ConfigError::Invalid("no models configured".into()));
        }
        if cfg.principals.is_empty() {
            return Err(ConfigError::Invalid("no principals configured".into()));
        }
        for (name, engine) in &cfg.engines {
            match engine.kind.as_str() {
                "mock" => {}
                "llama-cpp" => {
                    if engine.base_url.as_deref().unwrap_or("").is_empty() {
                        return Err(ConfigError::Invalid(format!(
                            "engine {name} needs base_url"
                        )));
                    }
                }
                other => {
                    return Err(ConfigError::Invalid(format!(
                        "engine {name} has unknown kind {other}"
                    )));
                }
            }
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_example_parses() {
        let text = include_str!("../../../examples/config/umbrad.yaml");
        let cfg = FileConfig::parse(text).expect("examples/config/umbrad.yaml");
        assert!(cfg.local_only);
        assert!(!cfg.allow_psi_override);
        assert!(cfg.aliases["default-chat"].cloud.is_none());
    }
}
