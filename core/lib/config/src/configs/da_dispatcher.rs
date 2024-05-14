use serde::Deserialize;

use crate::ObjectStoreConfig;

#[derive(Clone, Debug, PartialEq)]
pub struct DALayerInfo {
    pub name: String,
    pub private_key: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DataAvailabilityMode {
    DALayer(DALayerInfo),
    GCS(ObjectStoreConfig),
    NoDA,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DADispatcherConfig {
    pub mode: DataAvailabilityMode,
}

impl DADispatcherConfig {
    pub fn for_tests() -> Self {
        Self {
            mode: DataAvailabilityMode::DALayer(DALayerInfo {
                name: "zkDA".into(),
                private_key: vec![1, 2, 3],
            }),
        }
    }
}
