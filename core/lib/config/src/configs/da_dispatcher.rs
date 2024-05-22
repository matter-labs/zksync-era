use std::time::Duration;

use serde::Deserialize;

use crate::ObjectStoreConfig;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct DALayerInfo {
    pub name: String,
    #[serde(default)]
    pub private_key: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "da_mode")]
pub enum DataAvailabilityMode {
    DALayer(DALayerInfo),
    GCS(ObjectStoreConfig),
    NoDA,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DADispatcherConfig {
    #[serde(flatten)]
    pub da_mode: DataAvailabilityMode,
    pub polling_interval: Option<u32>,
    pub query_rows_limit: Option<u32>,
    pub max_retries: Option<u16>,
}

impl DADispatcherConfig {
    pub fn for_tests() -> Self {
        Self {
            da_mode: DataAvailabilityMode::DALayer(DALayerInfo {
                name: "zkDA".into(),
                private_key: "0x0".into(),
            }),
            polling_interval: Some(5),
            query_rows_limit: Some(100),
            max_retries: Some(5),
        }
    }

    pub fn polling_interval(&self) -> Duration {
        match self.polling_interval {
            Some(interval) => Duration::from_secs(interval as u64),
            None => Duration::from_secs(5),
        }
    }

    pub fn query_rows_limit(&self) -> u32 {
        self.query_rows_limit.unwrap_or(100)
    }

    pub fn max_retries(&self) -> u16 {
        self.max_retries.unwrap_or(5)
    }
}
