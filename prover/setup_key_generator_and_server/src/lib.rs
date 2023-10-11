use std::fs::File;
use std::io::Read;
use std::path::Path;
use anyhow::Context as _;

use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zkevm_test_harness::bellman::bn256::Bn256;
use zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zkevm_test_harness::witness::recursive_aggregation::padding_aggregations;
use zkevm_test_harness::witness::vk_set_generator::circuits_for_vk_generation;
use zksync_types::circuit::GEOMETRY_CONFIG;

use zksync_config::ProverConfigs;
use zksync_types::circuit::{LEAF_SPLITTING_FACTOR, NODE_SPLITTING_FACTOR, SCHEDULER_UPPER_BOUND};
pub fn get_setup_for_circuit_type(circuit_type: u8) -> anyhow::Result<Box<dyn Read>> {
    let filepath = get_setup_key_file_path(circuit_type).context("get_setup_key_file_path()")?;
    tracing::info!("Fetching setup key from path: {}", filepath);
    let file = File::open(filepath.clone())
        .with_context(|| format!("Failed reading setup key from path: {filepath}"))?;
    Ok(Box::new(file))
}

pub fn get_circuits_for_vk() -> anyhow::Result<Vec<ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>> {
    ensure_setup_key_exist().context("ensure_setup_key_exists()")?;
    let padding_aggregations = padding_aggregations(NODE_SPLITTING_FACTOR);
    Ok(circuits_for_vk_generation(
        GEOMETRY_CONFIG,
        LEAF_SPLITTING_FACTOR,
        NODE_SPLITTING_FACTOR,
        SCHEDULER_UPPER_BOUND,
        padding_aggregations,
    ))
}

fn ensure_setup_key_exist() -> anyhow::Result<()> {
    if !Path::new("setup_2^26.key").exists() {
        anyhow::bail!("File setup_2^26.key is required to be present in current directory.");
    }
    Ok(())
}

pub fn get_setup_key_write_file_path(circuit_type: u8) -> String {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| "/".into());
    format!("{}/{}", zksync_home, get_setup_key_filename(circuit_type))
}

fn get_setup_key_file_path(circuit_type: u8) -> anyhow::Result<String> {
    let prover_config = ProverConfigs::from_env().context("ProverConfigs::from_env()")?.non_gpu;
    Ok(format!(
        "{}/{}",
        prover_config.setup_keys_path,
        get_setup_key_filename(circuit_type)
    ))
}

fn get_setup_key_filename(circuit_type: u8) -> String {
    format!("setup_{}_key.bin", circuit_type)
}
