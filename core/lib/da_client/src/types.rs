use std::{error, fmt::Display};

use serde::Serialize;

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
    /// The blob_id is needed to fetch the inclusion data.
    pub blob_id: String,
}

impl From<String> for DispatchResponse {
    fn from(blob_id: String) -> Self {
        DispatchResponse { blob_id }
    }
}

/// `InclusionData` is the data needed to verify on L1 that a blob is included in the DA layer.
#[derive(Default, Serialize)]
pub struct InclusionData {
    /// The inclusion data serialized by the DA client. Serialization is done in a way that allows
    /// the deserialization of the data in Solidity contracts.
    pub data: Vec<u8>,
}
