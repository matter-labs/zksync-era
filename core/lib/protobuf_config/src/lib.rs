//! Defined protobuf mapping for the config files.
//! It allows to encode the configs using:
//! * protobuf binary format
//! * protobuf text format
//! * protobuf json format

mod alerts;
mod api;
mod chain;
mod contract_verifier;
mod contracts;
mod database;
mod eth_client;
mod eth_sender;
mod eth_watch;
mod fri_proof_compressor;
mod fri_prover;
mod fri_prover_gateway;
mod fri_prover_group;
mod fri_witness_generator;
mod fri_witness_vector_generator;
mod house_keeper;
mod native_token_fetcher;
mod object_store;
mod observability;
mod proof_data_handler;
mod snapshots_creator;
mod witness_generator;

pub mod proto;
pub mod testonly;
#[cfg(test)]
mod tests;
mod utils;

use anyhow::Context as _;
use zksync_types::{H160, H256};

fn parse_h256(bytes: &[u8]) -> anyhow::Result<H256> {
    Ok(<[u8; 32]>::try_from(bytes).context("invalid size")?.into())
}

fn parse_h160(bytes: &[u8]) -> anyhow::Result<H160> {
    Ok(<[u8; 20]>::try_from(bytes).context("invalid size")?.into())
}
