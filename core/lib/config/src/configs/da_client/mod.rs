use serde::Deserialize;

use crate::{AvailConfig, NearConfig, ObjectStoreConfig};

pub mod avail;
pub mod near;

pub const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
pub const NEAR_CLIENT_CONFIG_NAME: &str = "Near";
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
    Near(NearConfig),
}
