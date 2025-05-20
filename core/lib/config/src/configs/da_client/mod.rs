use serde::Deserialize;

use crate::{AvailConfig, CelestiaConfig, EigenDAConfig, ObjectStoreConfig};

pub mod avail;
pub mod celestia;
pub mod eigenda;

pub const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
pub const CELESTIA_CLIENT_CONFIG_NAME: &str = "Celestia";
pub const EIGENDA_CLIENT_CONFIG_NAME: &str = "EigenDA";
pub const OBJECT_STORE_CLIENT_CONFIG_NAME: &str = "ObjectStore";
pub const NO_DA_CLIENT_CONFIG_NAME: &str = "NoDA";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum DAClientConfig {
    Avail(AvailConfig),
    Celestia(CelestiaConfig),
    EigenDA(EigenDAConfig),
    ObjectStore(ObjectStoreConfig),
    NoDA,
}

impl From<AvailConfig> for DAClientConfig {
    fn from(config: AvailConfig) -> Self {
        Self::Avail(config)
    }
}
