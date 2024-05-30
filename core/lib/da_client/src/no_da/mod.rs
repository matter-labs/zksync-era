use std::fmt::Debug;

use async_trait::async_trait;
use zksync_da_layers::{
    types::{DispatchResponse, InclusionData},
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
    async fn dispatch_blob(&self, _: u32, _: Vec<u8>) -> Result<DispatchResponse, anyhow::Error> {
        Ok(DispatchResponse::default())
    }

    async fn get_inclusion_data(&self, _: String) -> Result<Option<InclusionData>, anyhow::Error> {
        return Ok(Some(InclusionData::default()));
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }
}
