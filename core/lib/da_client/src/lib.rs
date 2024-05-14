use std::fmt;

use zksync_types::L1BatchNumber;

use crate::types::{DispatchResponse, InclusionData};

pub mod clients;
mod types;

pub trait DataAvailabilityInterface: Sync + Send + fmt::Debug {
    fn dispatch_blob(
        &self,
        batch_number: L1BatchNumber,
        data: Vec<u8>,
    ) -> Result<DispatchResponse, types::Error>;
    fn get_inclusion_data(&self, blob_id: Vec<u8>) -> Result<InclusionData, types::Error>;
}
