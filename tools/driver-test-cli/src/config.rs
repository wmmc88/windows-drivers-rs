//! Configuration loading from TOML file.

use serde::Deserialize;
use std::path::Path;
use thiserror::Error;

pub const DEFAULT_CONFIG_FILE: &str = "driver-test.toml";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Deserialize, Default)]
pub struct VmConfig {
    pub name: Option<String>,
    pub cpus: Option<u8>,
    pub memory_mb: Option<u32>,
    pub baseline_snapshot: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Defaults {
    pub verbosity: Option<String>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub vm: Option<VmConfig>,
    pub defaults: Option<Defaults>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let data = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&data)?)
    }

    pub fn maybe_load(path: &Path) -> Option<Self> {
        if path.exists() {
            Self::load(path).ok()
        } else {
            None
        }
    }

    pub fn vm_name(&self) -> Option<&str> {
        self.vm.as_ref()?.name.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config() {
        let toml = r#"
[vm]
name = "test-vm"
cpus = 4
memory_mb = 4096
baseline_snapshot = "baseline"

[defaults]
verbosity = "info"
timeout_secs = 120
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.vm_name(), Some("test-vm"));
        assert_eq!(cfg.vm.unwrap().cpus, Some(4));
    }

    #[test]
    fn empty_config() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.vm_name(), None);
    }
}
