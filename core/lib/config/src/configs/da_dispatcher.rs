use std::time::Duration;

use serde::Deserialize;
use zksync_da_layers::config::DALayerConfig;

use crate::ObjectStoreConfig;

pub const DEFAULT_POLLING_INTERVAL_MS: u32 = 5000;
pub const DEFAULT_QUERY_ROWS_LIMIT: u32 = 100;
pub const DEFAULT_MAX_RETRIES: u16 = 5;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "da_mode")]
pub enum DataAvailabilityMode {
    /// Uses the data availability layer to dispatch pubdata.
    DALayer(DALayerConfig),
    /// Stores the pubdata in the Object Store(GCS/S3/...).
    ObjectStore(ObjectStoreConfig),
    /// Does not store the pubdata.
    NoDA,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DADispatcherConfig {
    /// The mode of the data availability layer. This defines the underlying client that will be
    /// used, and the configuration for that client.
    #[serde(flatten)]
    pub da_mode: DataAvailabilityMode,
    /// The interval at which the dispatcher will poll the DA layer for inclusion data.
    pub polling_interval_ms: Option<u32>,
    /// The maximum number of rows to query from the database in a single query.
    pub query_rows_limit: Option<u32>,
    /// The maximum number of retries for the dispatching of a blob.
    pub max_retries: Option<u16>,
}

impl DADispatcherConfig {
    pub fn for_tests() -> Self {
        Self {
            da_mode: DataAvailabilityMode::DALayer(DALayerConfig::Celestia(
                zksync_da_layers::clients::celestia::config::CelestiaConfig {
                    light_node_url: "localhost:12345".to_string(),
                    private_key: "0x0".to_string(),
                },
            )),
            polling_interval_ms: Some(DEFAULT_POLLING_INTERVAL_MS),
            query_rows_limit: Some(DEFAULT_QUERY_ROWS_LIMIT),
            max_retries: Some(DEFAULT_MAX_RETRIES),
        }
    }

    pub fn polling_interval(&self) -> Duration {
        match self.polling_interval_ms {
            Some(interval) => Duration::from_millis(interval as u64),
            None => Duration::from_millis(DEFAULT_POLLING_INTERVAL_MS as u64),
        }
    }

    pub fn query_rows_limit(&self) -> u32 {
        self.query_rows_limit.unwrap_or(DEFAULT_QUERY_ROWS_LIMIT)
    }

    pub fn max_retries(&self) -> u16 {
        self.max_retries.unwrap_or(DEFAULT_MAX_RETRIES)
    }
}
