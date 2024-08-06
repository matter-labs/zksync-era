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
mod da_dispatcher;
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
mod tee_verifier_input_producer;

mod external_price_api_client;
mod snapshot_recovery;
#[cfg(test)]
mod tests;
mod utils;
mod vm_runner;
mod wallets;

use std::{path::PathBuf, str::FromStr};

use anyhow::Context;
use zksync_protobuf::{
    build::{prost_reflect, prost_reflect::ReflectMessage, serde},
    ProtoRepr,
};
use zksync_types::{H160, H256};

fn parse_h256(bytes: &str) -> anyhow::Result<H256> {
    Ok(H256::from_str(bytes)?)
}

fn parse_h160(bytes: &str) -> anyhow::Result<H160> {
    Ok(H160::from_str(bytes)?)
}

pub fn read_optional_repr<P: ProtoRepr>(field: &Option<P>) -> Option<P::Type> {
    field
        .as_ref()
        .map(|x| x.read())
        .transpose()
        // This error will printed, only if the config partially filled, allows to debug config issues easier
        .map_err(|err| {
            tracing::error!("Failed to serialize config: {err}");
            err
        })
        .ok()
        .flatten()
}

pub fn decode_yaml_repr<T: ProtoRepr>(
    path: &PathBuf,
    deny_unknown_fields: bool,
) -> anyhow::Result<T::Type> {
    let yaml = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let d = serde_yaml::Deserializer::from_str(&yaml);
    let this: T = zksync_protobuf::serde::deserialize_proto_with_options(d, deny_unknown_fields)?;
    this.read()
}

pub fn encode_yaml_repr<T: ProtoRepr>(value: &T::Type) -> anyhow::Result<Vec<u8>> {
    let mut buffer = vec![];
    let mut s = serde_yaml::Serializer::new(&mut buffer);
    serialize_proto(&T::build(value), &mut s)?;
    Ok(buffer)
}

fn serialize_proto<T: ReflectMessage, S: serde::Serializer>(
    x: &T,
    s: S,
) -> Result<S::Ok, S::Error> {
    let opts = prost_reflect::SerializeOptions::new()
        .use_proto_field_name(true)
        .stringify_64_bit_integers(false);
    x.transcode_to_dynamic().serialize_with_options(s, &opts)
}
