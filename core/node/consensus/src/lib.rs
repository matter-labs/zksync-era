//! Consensus-related functionality.

#![allow(clippy::redundant_locals)]
#![allow(clippy::needless_pass_by_ref_mut)]

use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};

// Currently `batch` module is only used in tests,
// but will be used in production once batch syncing is implemented in consensus.
#[allow(unused)]
mod batch;
mod config;
mod en;
pub mod era;
mod mn;
mod storage;
#[cfg(test)]
pub(crate) mod testonly;
#[cfg(test)]
mod tests;
