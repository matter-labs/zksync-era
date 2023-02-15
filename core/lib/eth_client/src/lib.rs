#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

pub mod clients;
pub use clients::http_client::{ETHDirectClient, EthInterface};
