use eigen_da::EigenDAConfig;

use crate::{AvailConfig, ObjectStoreConfig};

pub mod avail;
pub mod eigen_da;

pub const EIGENDA_CLIENT_CONFIG_NAME: &str = "EigenDA";

pub const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
pub const OBJECT_STORE_CLIENT_CONFIG_NAME: &str = "ObjectStore";

#[derive(Debug, Clone, PartialEq)]
pub enum DAClientConfig {
    Avail(AvailConfig),
    ObjectStore(ObjectStoreConfig),
    EigenDA(EigenDAConfig),
}
