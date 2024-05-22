use std::{
    fmt,
    fmt::{Debug, Formatter},
};

use async_trait::async_trait;
use zksync_da_layers::{
    types::{DataAvailabilityError, DispatchResponse, InclusionData},
    DataAvailabilityInterface,
};

pub(crate) struct NoDAClient {}

impl NoDAClient {
    pub fn new() -> Self {
        NoDAClient {}
    }
}

#[async_trait]
impl DataAvailabilityInterface for NoDAClient {
    async fn dispatch_blob(
        &self,
        _: u32,
        _: Vec<u8>,
    ) -> Result<DispatchResponse, DataAvailabilityError> {
        Ok(DispatchResponse::default())
    }

    async fn get_inclusion_data(&self, _: Vec<u8>) -> Result<InclusionData, DataAvailabilityError> {
        return Ok(InclusionData::default());
    }
}

impl Debug for NoDAClient {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("NoDAClient").finish()
    }
}
