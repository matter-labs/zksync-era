//! Utilities for interacting with the zkSync L1 contract
//!
//! Provides utilities both to encode input data for the contract and to decode
//! the data provided by the contract.
//!
//! This crate utilizes traits provided by the `web3` crate to encode and decode
//! data. `Tokenizable` trait represents items that are encoded via single `Token`,
//! while `Tokenize` trait represents items that are encoded via array of `Token`s
//! (for example, transaction input).

pub use zksync_types::web3::contract::tokens::{Detokenize, Tokenizable, Tokenize};

/// Rust interface for (subset of) `IExector.sol`.
pub mod i_executor;
/// Utilities for interacting with `Multicall3` contract.
pub mod multicall3;
/// Utilities for interacting with the old verifier contract.
/// Required for backward compatibility only.
pub mod pre_boojum_verifier;
