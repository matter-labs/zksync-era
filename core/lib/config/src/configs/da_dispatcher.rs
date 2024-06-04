use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_POLLING_INTERVAL_MS: u32 = 5000;
pub const DEFAULT_QUERY_ROWS_LIMIT: u32 = 100;
pub const DEFAULT_MAX_RETRIES: u16 = 5;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DADispatcherConfig {
    pub polling_interval_ms: Option<u32>,
    /// The maximum number of rows to query from the database in a single query.
    pub query_rows_limit: Option<u32>,
    /// The maximum number of retries for the dispatching of a blob.
    pub max_retries: Option<u16>,
}

impl DADispatcherConfig {
    pub fn for_tests() -> Self {
        Self {
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
