#![allow(incomplete_features)] // We have to use generic const exprs.
#![feature(generic_const_exprs)]

pub mod basic_circuits;
pub mod leaf_aggregation;
pub mod node_aggregation;
pub mod precalculated_merkle_paths_provider;
pub mod recursion_tip;
pub mod scheduler;
mod storage_oracle;
pub mod utils;

pub mod metrics;

#[cfg(test)]
mod tests;
