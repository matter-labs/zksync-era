//! Consensus-related functionality.

#![allow(clippy::redundant_locals)]
#![allow(clippy::needless_pass_by_ref_mut)]

use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};

mod abi;
mod config;
mod en;
pub mod era;
mod mn;
mod registry;
mod storage;
#[cfg(test)]
pub(crate) mod testonly;
#[cfg(test)]
mod tests;
mod vm;
