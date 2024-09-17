use eigen_da::EigenDAConfig;
use serde::Deserialize;

use crate::{AvailConfig, ObjectStoreConfig};

pub mod avail;
pub mod eigen_da;

pub const EIGENDA_CLIENT_CONFIG_NAME: &str = "EigenDA";

pub const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
pub const OBJECT_STORE_CLIENT_CONFIG_NAME: &str = "ObjectStore";

#[derive(Debug, Clone, PartialEq)]
pub struct DAClientConfig {
    pub client: DAClient,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "client")]
pub enum DAClient {
    Avail(AvailConfig),
    ObjectStore(ObjectStoreConfig),
    EigenDA(EigenDAConfig),
}
