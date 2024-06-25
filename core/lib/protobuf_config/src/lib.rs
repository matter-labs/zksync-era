//! Defined protobuf mapping for the config files.
//! It allows to encode the configs using:
//! * protobuf binary format
//! * protobuf text format
//! * protobuf json format

mod api;
mod base_token_adjuster;
mod chain;
mod circuit_breaker;
mod commitment_generator;
mod consensus;
mod contract_verifier;
mod contracts;
mod database;
mod en;
mod eth;
mod experimental;
mod general;
mod genesis;
mod house_keeper;
mod object_store;
mod observability;
mod proof_data_handler;
pub mod proto;
mod prover;
mod pruning;
mod secrets;
mod snapshots_creator;

mod snapshot_recovery;
pub mod testonly;
#[cfg(test)]
mod tests;
mod utils;
mod vm_runner;
mod wallets;

use std::str::FromStr;

use zksync_protobuf::ProtoRepr;
use zksync_types::{H160, H256};

fn parse_h256(bytes: &str) -> anyhow::Result<H256> {
    Ok(H256::from_str(bytes)?)
}

fn parse_h160(bytes: &str) -> anyhow::Result<H160> {
    Ok(H160::from_str(bytes)?)
}

pub fn read_optional_repr<P: ProtoRepr>(field: &Option<P>) -> anyhow::Result<Option<P::Type>> {
    field.as_ref().map(|x| x.read()).transpose()
}
