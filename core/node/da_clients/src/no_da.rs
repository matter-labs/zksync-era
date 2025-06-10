use async_trait::async_trait;
use chrono::{DateTime, Utc};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
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

    async fn ensure_finality(
        &self,
        _: String,
        _: DateTime<Utc>,
    ) -> Result<Option<FinalityResponse>, DAError> {
        Ok(Some(FinalityResponse::default()))
    }

    async fn get_inclusion_data(&self, _: &str) -> Result<Option<InclusionData>, DAError> {
        Ok(Some(InclusionData::default()))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        None
    }

    fn client_type(&self) -> ClientType {
        ClientType::NoDA
    }

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(0)
    }
}
