//! Utilities for interacting with the zkSync L1 contract
//!
//! Provides utilities both to encode input data for the contract and to decode
//! the data provided by the contract.

use zksync_types::ethabi;

/// Rust interface for `IExector.sol`.
pub mod i_executor;
/// Utilities for interacting with the old verifier contract.
/// Reuired for backward compatibility only.
pub mod pre_boojum_verifier;

/// Allows to encode the input data as smart contract input.
///
/// Should not be used for types that logically represent a single token
/// (e.g. Solidity structures), use `From` trait for that.
pub trait ToEthArgs {
    fn to_eth_args(&self) -> Vec<ethabi::Token>;
}

/// Allows to decode the input data from the smart contract input.
pub trait FromTokens {
    /// The error type that can be returned during the decoding.
    type Error;

    /// Decodes the data from the `ethabi::Token` representation.
    fn from_tokens(tokens: &[ethabi::Token]) -> Result<Self, Self::Error>
    where
        Self: Sized;
}
