use std::{error, fmt::Display};

use serde::Serialize;
use zksync_types::commitment::PubdataType;

/// `DAError` is the error type returned by the DA clients.
#[derive(Debug)]
pub struct DAError {
    pub error: anyhow::Error,
    pub is_retriable: bool,
}

impl DAError {
    pub fn is_retriable(&self) -> bool {
        self.is_retriable
    }
}

impl Display for DAError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = if self.is_retriable {
            "retriable"
        } else {
            "fatal"
        };
        write!(f, "{kind} data availability client error: {}", self.error)
    }
}

impl error::Error for DAError {}

/// `DispatchResponse` is the response received from the DA layer after dispatching a blob.
#[derive(Default)]
pub struct DispatchResponse {
    /// The request_id is needed to fetch the inclusion data.
    pub request_id: String,
}

impl From<String> for DispatchResponse {
    fn from(request_id: String) -> Self {
        DispatchResponse { request_id }
    }
}

#[derive(Default)]
pub struct FinalityResponse {
    pub blob_id: String,
}

/// `InclusionData` is the data needed to verify on L1 that a blob is included in the DA layer.
#[derive(Default, Serialize)]
pub struct InclusionData {
    /// The inclusion data serialized by the DA client. Serialization is done in a way that allows
    /// the deserialization of the data in Solidity contracts.
    pub data: Vec<u8>,
}

pub enum ClientType {
    NoDA,
    Avail,
    Celestia,
    EigenDA,
    ObjectStore,
}

impl ClientType {
    pub fn into_pubdata_type(self) -> PubdataType {
        match self {
            ClientType::NoDA => PubdataType::NoDA,
            ClientType::Avail => PubdataType::Avail,
            ClientType::Celestia => PubdataType::Celestia,
            ClientType::EigenDA => PubdataType::Eigen,
            ClientType::ObjectStore => PubdataType::ObjectStore,
        }
    }
}
