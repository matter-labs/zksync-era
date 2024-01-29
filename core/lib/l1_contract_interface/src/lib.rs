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
    ///
    /// Note: vector of tokens is interpreted differently from `Token::Tuple` variant.
    /// Vector may be used to encode several arguments that represent an input of some function,
    /// while `Tuple` represents a single argument that is a combination of several tokens.
    /// If you need to encode some structure, you should return a vector with a single `Tuple` token,
    /// and if you're encoding function input, you probably need to return a vector of tokens.
    fn into_tokens(self) -> Vec<ethabi::Token>;
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
