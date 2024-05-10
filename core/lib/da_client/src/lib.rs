use std::fmt;

use crate::types::{DispatchResponse, InclusionData};

pub mod clients;
mod types;

pub trait DataAvailabilityClient<E: fmt::Display>: Sync + Send + fmt::Debug {
    fn dispatch_blob(&self, data: Vec<u8>) -> Result<DispatchResponse, E>;
    fn get_inclusion_data(&self, blob_id: Vec<u8>) -> Result<InclusionData, E>;
}
