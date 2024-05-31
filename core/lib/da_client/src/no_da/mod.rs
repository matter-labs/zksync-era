use std::fmt::Debug;

use async_trait::async_trait;
use zksync_da_layers::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};

/// A no-op implementation of the `DataAvailabilityClient` trait, that doesn't store the pubdata.
#[derive(Clone, Debug, Default)]
pub struct NoDAClient;

impl NoDAClient {
    pub fn new() -> Self {
        NoDAClient {}
    }
}

#[async_trait]
impl DataAvailabilityClient for NoDAClient {
    async fn dispatch_blob(&self, _: u32, _: Vec<u8>) -> Result<DispatchResponse, DAError> {
        Ok(DispatchResponse::default())
    }

    async fn get_inclusion_data(&self, _: String) -> Result<Option<InclusionData>, DAError> {
        return Ok(Some(InclusionData::default()));
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> usize {
        100 * 1024 * 1024 // 100 MB, high enough to not be a problem
    }
}
