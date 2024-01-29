//! Utilities for interacting with the zkSync L1 contract
//!
//! Provides utilities both to encode input data for the contract and to decode
//! the data provided by the contract.
//!
//! Most of the structures defined in this module are wrappers around the
//! `zksync_types` crate to align the internally used types with the types
//! expected by the contract.

/// Public reexport of `ethabi` version used by this crate.
pub use zksync_types::ethabi;

/// Rust interface for `IExector.sol`.
pub mod i_executor;

/// Allows to encode the input data as smart contract input.
pub trait IntoTokens {
    /// Transforms the data into the `ethabi::Token` representation.
    fn into_tokens(self) -> ethabi::Token;
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
