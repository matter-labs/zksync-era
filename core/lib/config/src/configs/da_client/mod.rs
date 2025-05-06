use serde::Deserialize;

use crate::{AvailConfig, CelestiaConfig, EigenConfigV2M0, ObjectStoreConfig};

pub mod avail;
pub mod celestia;
pub mod eigenv2m0;

pub const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
pub const CELESTIA_CLIENT_CONFIG_NAME: &str = "Celestia";
pub const EIGENV2M0_CLIENT_CONFIG_NAME: &str = "EigenV2M0";
pub const OBJECT_STORE_CLIENT_CONFIG_NAME: &str = "ObjectStore";
pub const NO_DA_CLIENT_CONFIG_NAME: &str = "NoDA";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum DAClientConfig {
    Avail(AvailConfig),
    Celestia(CelestiaConfig),
    EigenV2M0(EigenConfigV2M0),
    ObjectStore(ObjectStoreConfig),
    NoDA,
}

impl From<AvailConfig> for DAClientConfig {
    fn from(config: AvailConfig) -> Self {
        Self::Avail(config)
    }
}
