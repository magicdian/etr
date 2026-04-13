use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EtrConfig {
    pub service: ServiceConfig,
    pub management: ManagementConfig,
    pub data_plane: DataPlaneConfig,
    #[serde(default)]
    pub rules: Vec<ForwardRule>,
}

impl EtrConfig {
    pub fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;

        Self::from_toml_str(&raw)
    }

    pub fn from_toml_str(raw: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(raw).map_err(ConfigError::Parse)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.rules.is_empty() {
            return Err(ConfigError::EmptyRules);
        }

        if self.service.node_name.trim().is_empty() {
            return Err(ConfigError::EmptyNodeName);
        }

        if self.data_plane.external_interface.trim().is_empty() {
            return Err(ConfigError::EmptyExternalInterface);
        }

        let mut seen_frontends = HashSet::new();

        for rule in &self.rules {
            if rule.name.trim().is_empty() {
                return Err(ConfigError::EmptyRuleName);
            }

            if !rule.enabled {
                continue;
            }

            if rule.family != AddressFamily::Ipv4 {
                return Err(ConfigError::UnsupportedFamily(
                    rule.name.clone(),
                    rule.family,
                ));
            }

            if !matches!(rule.listen_addr, IpAddr::V4(_)) {
                return Err(ConfigError::FamilyMismatch(
                    rule.name.clone(),
                    "listen_addr must be IPv4 when family = ipv4".to_owned(),
                ));
            }

            if rule.backends.len() != 1 {
                return Err(ConfigError::InvalidBackendCount(
                    rule.name.clone(),
                    rule.backends.len(),
                ));
            }

            let backend = &rule.backends[0];
            if !matches!(backend.addr, IpAddr::V4(_)) {
                return Err(ConfigError::FamilyMismatch(
                    rule.name.clone(),
                    "backend addr must be IPv4 in the MVP".to_owned(),
                ));
            }

            let frontend_key = (rule.protocol, rule.listen_addr, rule.listen_port);
            if !seen_frontends.insert(frontend_key) {
                return Err(ConfigError::DuplicateFrontend(
                    rule.protocol,
                    rule.listen_addr,
                    rule.listen_port,
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    pub node_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagementConfig {
    pub listen: SocketAddr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DataPlaneConfig {
    #[serde(default = "default_data_plane_kind")]
    pub kind: DataPlaneKind,
    pub external_interface: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DataPlaneKind {
    Tc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ForwardRule {
    pub name: String,
    pub protocol: Protocol,
    pub family: AddressFamily,
    pub listen_addr: IpAddr,
    pub listen_port: u16,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub snat: SnatMode,
    #[serde(default)]
    pub backends: Vec<BackendTarget>,
}

impl ForwardRule {
    pub fn frontend_label(&self) -> String {
        format!(
            "{}://{}:{}",
            self.protocol, self.listen_addr, self.listen_port
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BackendTarget {
    pub addr: IpAddr,
    pub port: u16,
    #[serde(default = "default_weight")]
    pub weight: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}

impl Display for Protocol {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp => f.write_str("tcp"),
            Self::Udp => f.write_str("udp"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AddressFamily {
    Ipv4,
    Ipv6,
}

impl Display for AddressFamily {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ipv4 => f.write_str("ipv4"),
            Self::Ipv6 => f.write_str("ipv6"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SnatMode {
    #[default]
    Masquerade,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse TOML config: {0}")]
    Parse(toml::de::Error),
    #[error("config must contain at least one rule")]
    EmptyRules,
    #[error("service.node_name must not be empty")]
    EmptyNodeName,
    #[error("data_plane.external_interface must not be empty")]
    EmptyExternalInterface,
    #[error("rule name must not be empty")]
    EmptyRuleName,
    #[error("rule '{0}' must define exactly one backend in the MVP, found {1}")]
    InvalidBackendCount(String, usize),
    #[error("rule '{0}' uses unsupported address family '{1}' for the MVP")]
    UnsupportedFamily(String, AddressFamily),
    #[error("rule '{0}' has invalid address-family mapping: {1}")]
    FamilyMismatch(String, String),
    #[error("duplicate frontend rule for {0}://{1}:{2}")]
    DuplicateFrontend(Protocol, IpAddr, u16),
}

fn default_enabled() -> bool {
    true
}

fn default_weight() -> u16 {
    1
}

fn default_data_plane_kind() -> DataPlaneKind {
    DataPlaneKind::Tc
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CONFIG: &str = r#"
[service]
node_name = "edge-hk-01"

[management]
listen = "127.0.0.1:9911"

[data_plane]
kind = "tc"
external_interface = "eth0"

[[rules]]
name = "ssh-proxy"
protocol = "tcp"
family = "ipv4"
listen_addr = "0.0.0.0"
listen_port = 16020
snat = "masquerade"

[[rules.backends]]
addr = "163.223.125.6"
port = 11426
"#;

    #[test]
    fn parses_valid_config() {
        let config = EtrConfig::from_toml_str(VALID_CONFIG).expect("config should parse");

        assert_eq!(config.service.node_name, "edge-hk-01");
        assert_eq!(config.data_plane.external_interface, "eth0");
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].protocol, Protocol::Tcp);
        assert_eq!(config.rules[0].backends.len(), 1);
    }

    #[test]
    fn rejects_ipv6_mvp_rules() {
        let invalid = VALID_CONFIG.replace("family = \"ipv4\"", "family = \"ipv6\"");

        let error = EtrConfig::from_toml_str(&invalid).expect_err("ipv6 should be rejected");
        assert!(matches!(
            error,
            ConfigError::UnsupportedFamily(_, AddressFamily::Ipv6)
        ));
    }

    #[test]
    fn rejects_multi_backend_mvp_rules() {
        let invalid =
            format!("{VALID_CONFIG}\n[[rules.backends]]\naddr = \"163.223.125.7\"\nport = 11427\n");

        let error =
            EtrConfig::from_toml_str(&invalid).expect_err("multi backend should be rejected");
        assert!(matches!(error, ConfigError::InvalidBackendCount(_, 2)));
    }
}
