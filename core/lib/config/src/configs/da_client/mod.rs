use std::{fmt::Display, string::ToString};

use crate::{AvailConfig, CelestiaConfig, EigenConfig, ObjectStoreConfig};

pub mod avail;
pub mod celestia;
pub mod eigen;

pub const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
pub const CELESTIA_CLIENT_CONFIG_NAME: &str = "Celestia";
pub const EIGEN_CLIENT_CONFIG_NAME: &str = "Eigen";
pub const OBJECT_STORE_CLIENT_CONFIG_NAME: &str = "ObjectStore";
pub const NO_DA_CLIENT_CONFIG_NAME: &str = "NoDA";

#[derive(Debug, Clone, PartialEq)]
pub enum DAClientConfig {
    Avail(AvailConfig),
    Celestia(CelestiaConfig),
    Eigen(EigenConfig),
    ObjectStore(ObjectStoreConfig),
}

impl From<AvailConfig> for DAClientConfig {
    fn from(config: AvailConfig) -> Self {
        Self::Avail(config)
    }
}

impl Display for DAClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            DAClientConfig::Avail(_) => AVAIL_CLIENT_CONFIG_NAME.to_string(),
            DAClientConfig::Celestia(_) => CELESTIA_CLIENT_CONFIG_NAME.to_string(),
            DAClientConfig::Eigen(_) => EIGEN_CLIENT_CONFIG_NAME.to_string(),
            DAClientConfig::ObjectStore(_) => OBJECT_STORE_CLIENT_CONFIG_NAME.to_string(),
        };
        write!(f, "{}", str)
    }
}
