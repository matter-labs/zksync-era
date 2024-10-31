use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct L1RecoveryConfig {
    pub enabled: bool,
}
