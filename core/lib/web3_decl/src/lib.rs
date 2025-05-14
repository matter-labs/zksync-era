//! `zksync_web3_decl` is a collection of common types required for ZKsync Web3 API
//! and also `jsonrpsee`-based declaration of server and client traits.
//!
//! Web3 namespaces are declared in `namespaces` module.
//!
//! For the usage of these traits, check the documentation of `jsonrpsee` crate.

#![allow(clippy::derive_partial_eq_without_eq)]

pub mod client;
#[cfg(feature = "node_framework")]
pub mod di;
pub mod error;
pub mod namespaces;
pub mod types;

// Re-export to simplify crate usage (especially for server implementations).
pub use jsonrpsee;
