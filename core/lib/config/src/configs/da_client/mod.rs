use eigen_da::EigenDAConfig;

use crate::{AvailConfig, CelestiaConfig, EigenConfig, ObjectStoreConfig};

pub mod avail;
pub mod celestia;
pub mod eigen;
pub mod eigen_da;

pub const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
pub const CELESTIA_CLIENT_CONFIG_NAME: &str = "Celestia";
pub const EIGEN_CLIENT_CONFIG_NAME: &str = "Eigen";
pub const EIGENDA_CLIENT_CONFIG_NAME: &str = "EigenDA";
pub const OBJECT_STORE_CLIENT_CONFIG_NAME: &str = "ObjectStore";

#[derive(Debug, Clone, PartialEq)]
pub enum DAClientConfig {
    Avail(AvailConfig),
    Celestia(CelestiaConfig),
    Eigen(EigenConfig),
    ObjectStore(ObjectStoreConfig),
    EigenDA(EigenDAConfig),
}
