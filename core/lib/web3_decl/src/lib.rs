//! `zksync_web3_decl` is a collection of common types required for zkSync Web3 API
//! and also `jsonrpsee`-based declaration of server and client traits.
//!
//! Web3 namespaces are declared in `namespaces` module.
//!
//! For the usage of these traits, check the documentation of `jsonrpsee` crate.

#![allow(clippy::derive_partial_eq_without_eq)]

#[cfg(all(not(feature = "server"), not(feature = "client")))]
std::compile_error!(r#"At least on of features ["server", "client"] must be enabled"#);

pub mod error;
pub mod namespaces;
pub mod types;

pub use jsonrpsee;
use jsonrpsee::core::Error;

pub type RpcResult<T> = Result<T, Error>;
