use serde::Deserialize;

use crate::ObjectStoreConfig;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct DALayerInfo {
    pub name: String,
    #[serde(default)]
    pub private_key: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum DataAvailabilityMode {
    DALayer(DALayerInfo),
    GCS(ObjectStoreConfig),
    NoDA,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
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
