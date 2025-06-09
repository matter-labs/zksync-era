//! Point of interaction of the core subsystem with the prover subsystem.
//! Defines the means of communication between the two subsystems without exposing the internal details of either.

use serde::{Deserialize, Serialize};

/// Types that define the API for interaction between prover and server subsystems.
pub mod api;
/// Inputs for proof generation provided by the core subsystem.
pub mod inputs;
pub mod legacy;
/// Outputs of proof generation provided by the prover subsystem.
pub mod outputs;
pub mod proving_network;

// Marker trait for the serialization format of stored data.
pub trait FormatMarker: private::Sealed {}

// We don't want for some other crate to implement this trait for their own types,
// so we seal it.
mod private {
    pub trait Sealed {}
}

// Marker struct for CBOR serialization format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CBOR;

// Marker struct for Bincode serialization format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bincode;

impl private::Sealed for CBOR {}
impl private::Sealed for Bincode {}

impl FormatMarker for CBOR {}
impl FormatMarker for Bincode {}
