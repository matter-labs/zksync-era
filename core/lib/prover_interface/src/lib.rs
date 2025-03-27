//! Point of interaction of the core subsystem with the prover subsystem.
//! Defines the means of communication between the two subsystems without exposing the internal details of either.

/// Types that define the API for interaction between prover and server subsystems.
pub mod api;
/// Inputs for proof generation provided by the core subsystem.
pub mod inputs;
/// Outputs of proof generation provided by the prover subsystem.
pub mod outputs;
