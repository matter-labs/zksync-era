use async_trait::async_trait;
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};

/// A no-op implementation of the `DataAvailabilityClient` trait, that doesn't store the pubdata.
#[derive(Clone, Debug, Default)]
pub struct NoDAClient;

#[async_trait]
impl DataAvailabilityClient for NoDAClient {
    async fn dispatch_blob(&self, _: u32, _: Vec<u8>) -> Result<DispatchResponse, DAError> {
        Ok(DispatchResponse::default())
    }

    async fn get_inclusion_data(&self, _: &str) -> Result<Option<InclusionData>, DAError> {
        return Ok(Some(InclusionData::default()));
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        None
    }
}
