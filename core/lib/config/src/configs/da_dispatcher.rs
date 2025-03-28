use std::time::Duration;

use serde::Deserialize;

/// The default interval between the `da_dispatcher's` iterations.
pub const DEFAULT_POLLING_INTERVAL_MS: u32 = 5000;
/// The maximum number of rows to fetch from the database in a single query. The value has to be
/// not too high to avoid the dispatcher iteration taking too much time.
pub const DEFAULT_MAX_ROWS_TO_DISPATCH: u32 = 3;
/// The maximum number of retries for the dispatch of a blob.
pub const DEFAULT_MAX_RETRIES: u16 = 5;
/// Use dummy value as inclusion proof instead of getting it from the client.
pub const DEFAULT_USE_DUMMY_INCLUSION_DATA: bool = false;
/// The default value for the inclusion_verification_transition_enabled flag.
pub const DEFAULT_INCLUSION_VERIFICATION_TRANSITION_ENABLED: bool = false;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DADispatcherConfig {
    /// The interval between the `da_dispatcher's` iterations.
    pub polling_interval_ms: Option<u32>,
    /// The maximum number of rows to query from the database in a single query.
    pub max_rows_to_dispatch: Option<u32>,
    /// The maximum number of retries for the dispatch of a blob.
    pub max_retries: Option<u16>,
    /// Use dummy value as inclusion proof instead of getting it from the client.
    pub use_dummy_inclusion_data: Option<bool>,
    /// This flag is used to signal that the transition to the full DA validators is in progress.
    /// It will make the dispatcher stop polling for inclusion data and ensure all the old batches
    /// have at least dummy inclusion data.
    pub inclusion_verification_transition_enabled: Option<bool>,
}

impl DADispatcherConfig {
    pub fn for_tests() -> Self {
        Self {
            polling_interval_ms: Some(DEFAULT_POLLING_INTERVAL_MS),
            max_rows_to_dispatch: Some(DEFAULT_MAX_ROWS_TO_DISPATCH),
            max_retries: Some(DEFAULT_MAX_RETRIES),
            use_dummy_inclusion_data: Some(DEFAULT_USE_DUMMY_INCLUSION_DATA),
            inclusion_verification_transition_enabled: Some(
                DEFAULT_INCLUSION_VERIFICATION_TRANSITION_ENABLED,
            ),
        }
    }

    pub fn polling_interval(&self) -> Duration {
        match self.polling_interval_ms {
            Some(interval) => Duration::from_millis(interval as u64),
            None => Duration::from_millis(DEFAULT_POLLING_INTERVAL_MS as u64),
        }
    }

    pub fn max_rows_to_dispatch(&self) -> u32 {
        self.max_rows_to_dispatch
            .unwrap_or(DEFAULT_MAX_ROWS_TO_DISPATCH)
    }

    pub fn max_retries(&self) -> u16 {
        self.max_retries.unwrap_or(DEFAULT_MAX_RETRIES)
    }

    pub fn use_dummy_inclusion_data(&self) -> bool {
        self.use_dummy_inclusion_data
            .unwrap_or(DEFAULT_USE_DUMMY_INCLUSION_DATA)
    }

    pub fn inclusion_verification_transition_enabled(&self) -> bool {
        self.inclusion_verification_transition_enabled
            .unwrap_or(DEFAULT_INCLUSION_VERIFICATION_TRANSITION_ENABLED)
    }
}
