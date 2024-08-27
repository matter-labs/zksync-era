use serde::Deserialize;

use crate::{AvailConfig, ObjectStoreConfig};

pub mod avail;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DAClientConfig {
    #[serde(flatten)]
    pub client: DAClient,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "client")]
pub enum DAClient {
    NoDA,
    Avail(AvailConfig),
    ObjectStore(ObjectStoreConfig),
}
