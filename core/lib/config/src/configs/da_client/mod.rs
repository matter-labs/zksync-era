use crate::{AvailConfig, ObjectStoreConfig};

pub mod avail;

pub const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
pub const OBJECT_STORE_CLIENT_CONFIG_NAME: &str = "ObjectStore";

#[derive(Debug, Clone, PartialEq)]
pub enum DAClientConfig {
    Avail(AvailConfig),
    ObjectStore(ObjectStoreConfig),
}
