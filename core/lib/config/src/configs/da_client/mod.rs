use serde::Deserialize;

use crate::{AvailConfig, CelestiaConfig, EigenConfigV1M0, EigenConfigV2M0, EigenConfigV2M1, ObjectStoreConfig};

pub mod avail;
pub mod celestia;
pub mod eigenv1m0;
pub mod eigenv2m1;
pub mod eigenv2m0;

pub const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
pub const CELESTIA_CLIENT_CONFIG_NAME: &str = "Celestia";
pub const EIGENV1M0_CLIENT_CONFIG_NAME: &str = "EigenV1M0";
pub const EIGENV2M0_CLIENT_CONFIG_NAME: &str = "EigenV2M0";
pub const EIGENV2M1_CLIENT_CONFIG_NAME: &str = "EigenV2M1";
pub const OBJECT_STORE_CLIENT_CONFIG_NAME: &str = "ObjectStore";
pub const NO_DA_CLIENT_CONFIG_NAME: &str = "NoDA";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum DAClientConfig {
    Avail(AvailConfig),
    Celestia(CelestiaConfig),
    EigenV1M0(EigenConfigV1M0),
    EigenV2M0(EigenConfigV2M0),
    EigenV2M1(EigenConfigV2M1),
    ObjectStore(ObjectStoreConfig),
    NoDA,
}

impl From<AvailConfig> for DAClientConfig {
    fn from(config: AvailConfig) -> Self {
        Self::Avail(config)
    }
}
