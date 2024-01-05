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

pub mod proto;
mod repr;
#[cfg(test)]
mod tests;
mod utils;

use zksync_types::{H160,H256};
use anyhow::Context as _;

fn parse_h256(bytes: &[u8]) -> anyhow::Result<H256> {
    Ok(<[u8; 32]>::try_from(bytes).context("invalid size")?.into())
}

fn parse_h160(bytes: &[u8]) -> anyhow::Result<H160> {
    Ok(<[u8; 20]>::try_from(bytes).context("invalid size")?.into())
}
