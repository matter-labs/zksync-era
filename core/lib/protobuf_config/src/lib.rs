//! Defined protobuf mapping for the config files.
//! It allows to encode the configs using:
//! * protobuf binary format
//! * protobuf text format
//! * protobuf json format

mod api;
mod chain;
mod contract_verifier;
mod contracts;
mod database;
mod eth;
mod house_keeper;
mod object_store;
mod observability;
mod proof_data_handler;
mod snapshots_creator;
mod witness_generator;

mod circuit_breaker;
mod general;
mod genesis;
pub mod proto;
mod prover;
pub mod testonly;
#[cfg(test)]
mod tests;
mod utils;
mod wallets;

use anyhow::Context as _;
use zksync_protobuf::ProtoRepr;
use zksync_types::{H160, H256};

fn parse_h256(bytes: &[u8]) -> anyhow::Result<H256> {
    Ok(<[u8; 32]>::try_from(bytes).context("invalid size")?.into())
}

fn parse_h160(bytes: &[u8]) -> anyhow::Result<H160> {
    Ok(<[u8; 20]>::try_from(bytes).context("invalid size")?.into())
}

fn read_optional_repr<P: ProtoRepr>(field: &Option<P>) -> anyhow::Result<Option<P::Type>> {
    field.as_ref().map(|x| x.read()).transpose()
}
