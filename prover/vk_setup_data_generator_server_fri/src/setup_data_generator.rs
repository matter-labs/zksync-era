use std::collections::HashMap;

use anyhow::Context as _;
use circuit_definitions::{
    circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType,
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};
use zkevm_test_harness::{
    geometry_config::get_geometry_config,
    prover_utils::{create_base_layer_setup_data, create_recursive_layer_setup_data},
};
use zksync_prover_fri_types::{
    circuit_definitions::{
        aux_definitions::witness_oracle::VmWitnessOracle,
        boojum::{field::goldilocks::GoldilocksField, worker::Worker},
        circuit_definitions::{
            base_layer::ZkSyncBaseLayerCircuit, recursion_layer::ZkSyncRecursiveLayerCircuit,
        },
        ZkSyncDefaultRoundFunction, BASE_LAYER_CAP_SIZE, BASE_LAYER_FRI_LDE_FACTOR,
    },
    ProverServiceDataKey,
};
use zksync_types::basic_fri_types::AggregationRound;
#[cfg(feature = "gpu")]
use {crate::GpuProverSetupData, shivini::cs::setup::GpuSetup, shivini::ProverContext};

use crate::{
    keystore::Keystore,
    utils::{
        get_basic_circuits, get_leaf_circuits, get_node_circuit, get_scheduler_circuit, CYCLE_LIMIT,
    },
    GoldilocksProverSetupData, ProverSetupData,
};

fn generate_all(
    keystore: &Keystore,
    dry_run: bool,
    f: fn(&Keystore, bool, u8, bool) -> anyhow::Result<String>,
) -> anyhow::Result<HashMap<String, String>> {
    let mut result = HashMap::new();

    for numeric_circuit in
        BaseLayerCircuitType::VM as u8..=BaseLayerCircuitType::L1MessagesHasher as u8
    {
        let digest = f(keystore, true, numeric_circuit, dry_run)
            .context(format!("base layer, circuit {:?}", numeric_circuit))?;
        result.insert(format!("base_{}", numeric_circuit), digest);
    }

    // +2 - as '1' and '2' are scheduler and node respectively.
    for numeric_circuit in ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8
        ..=ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8
    {
        let digest = f(keystore, false, numeric_circuit, dry_run)
            .context(format!("recursive layer, circuit {:?}", numeric_circuit))?;
        result.insert(format!("recursive_{}", numeric_circuit), digest);
    }
    Ok(result)
}

pub fn generate_all_cpu_setup_data(
    keystore: &Keystore,
    dry_run: bool,
) -> anyhow::Result<HashMap<String, String>> {
    generate_all(keystore, dry_run, generate_cpu_setup_data)
}

pub fn generate_all_gpu_setup_data(
    keystore: &Keystore,
    dry_run: bool,
) -> anyhow::Result<HashMap<String, String>> {
    generate_all(keystore, dry_run, generate_gpu_setup_data)
}

pub fn generate_cpu_setup_data(
    keystore: &Keystore,
    is_base_layer: bool,
    numeric_circuit: u8,
    dry_run: bool,
) -> anyhow::Result<String> {
    match is_base_layer {
        true => {
            let circuit = get_base_layer_circuit(keystore, numeric_circuit)
                .context("get_base_layer_circuit")?;
            let prover_setup_data = generate_cpu_base_layer_setup_data(keystore, circuit)
                .context("generate_cpu_base_layer_setup_data()")?;
            // Serialization should always succeed.
            let serialized =
                bincode::serialize(&prover_setup_data).expect("Failed serializing setup data");
            let digest = md5::compute(&serialized);

            if !dry_run {
                keystore
                    .save_setup_data_for_circuit_type(
                        ProverServiceDataKey::new(numeric_circuit, AggregationRound::BasicCircuits),
                        &serialized,
                    )
                    .context("save_setup_data_for_circuit_type()")?;
            } else {
                tracing::warn!("Dry run - not writing the key");
            }
            Ok(format!("{:?}", digest))
        }
        false => {
            let circuit = get_recursive_circuit(keystore, numeric_circuit)
                .context("get_recursive_circuit()")?;
            let prover_setup_data = generate_cpu_recursive_layer_setup_data(keystore, circuit)
                .context("generate_cpu_recursive_layer_setup_data()")?;
            // Serialization should always succeed.
            let serialized =
                bincode::serialize(&prover_setup_data).expect("Failed serializing setup data");

            let digest = md5::compute(&serialized);
            if !dry_run {
                keystore
                    .save_setup_data_for_circuit_type(
                        ProverServiceDataKey::new_recursive(numeric_circuit),
                        &serialized,
                    )
                    .context("save_setup_data_for_circuit_type()")?;
            } else {
                tracing::warn!("Dry run - not writing the key");
            }
            Ok(format!("{:?}", digest))
        }
    }
}

fn get_base_layer_circuit(
    keystore: &Keystore,
    id: u8,
) -> anyhow::Result<
    ZkSyncBaseLayerCircuit<
        GoldilocksField,
        VmWitnessOracle<GoldilocksField>,
        ZkSyncDefaultRoundFunction,
    >,
> {
    get_basic_circuits(keystore, CYCLE_LIMIT, get_geometry_config())
        .context("get_basic_circuits()")?
        .into_iter()
        .find(|circuit| id == circuit.numeric_circuit_type())
        .with_context(|| format!("No basic circuit found for id: {id}"))
}

fn get_recursive_circuit(
    keystore: &Keystore,
    id: u8,
) -> anyhow::Result<ZkSyncRecursiveLayerCircuit> {
    let mut recursive_circuits = get_leaf_circuits(keystore).context("get_leaf_circuits()")?;
    recursive_circuits.push(get_node_circuit(keystore).context("get_node_circuit()")?);
    recursive_circuits.push(get_scheduler_circuit(keystore).context("get_scheduler_circuit()")?);
    recursive_circuits
        .into_iter()
        .find(|circuit| id == circuit.numeric_circuit_type())
        .with_context(|| format!("No recursive circuit found for id: {id}"))
}

pub fn generate_cpu_base_layer_setup_data(
    keystore: &Keystore,
    circuit: ZkSyncBaseLayerCircuit<
        GoldilocksField,
        VmWitnessOracle<GoldilocksField>,
        ZkSyncDefaultRoundFunction,
    >,
) -> anyhow::Result<GoldilocksProverSetupData> {
    let circuit_type = circuit.numeric_circuit_type();
    tracing::info!(
        "starting setup data generator for base layer circuit: {}.",
        circuit_type
    );
    let worker = Worker::new();
    let (setup_base, setup, vk, setup_tree, vars_hint, wits_hint, finalization_hint) =
        create_base_layer_setup_data(
            circuit.clone(),
            &worker,
            BASE_LAYER_FRI_LDE_FACTOR,
            BASE_LAYER_CAP_SIZE,
        );
    let key = ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits);
    let existing_finalization_hint = keystore
        .load_finalization_hints(key)
        .context("load_finalization_hints()")?;
    if existing_finalization_hint != finalization_hint {
        anyhow::bail!("finalization hint mismatch for circuit: {circuit_type}");
    }
    let existing_vk = keystore
        .load_base_layer_verification_key(circuit_type)
        .with_context(|| format!("load_base_layer_verification_key({circuit_type})"))?;
    if existing_vk.into_inner() != vk {
        anyhow::bail!("vk mismatch for circuit: {circuit_type}");
    }
    Ok(ProverSetupData {
        setup_base,
        setup,
        vk,
        setup_tree,
        vars_hint,
        wits_hint,
        finalization_hint,
    })
}

fn generate_cpu_recursive_layer_setup_data(
    keystore: &Keystore,
    circuit: ZkSyncRecursiveLayerCircuit,
) -> anyhow::Result<GoldilocksProverSetupData> {
    let circuit_type = circuit.numeric_circuit_type();
    tracing::info!(
        "starting setup data generator for recursive layer circuit: {}.",
        circuit_type
    );
    let worker = Worker::new();
    let (setup_base, setup, vk, setup_tree, vars_hint, wits_hint, finalization_hint) =
        create_recursive_layer_setup_data(
            circuit.clone(),
            &worker,
            BASE_LAYER_FRI_LDE_FACTOR,
            BASE_LAYER_CAP_SIZE,
        );
    let key = ProverServiceDataKey::new_recursive(circuit_type);
    let existing_finalization_hint = keystore
        .load_finalization_hints(key)
        .context("load_finalization_hints()")?;
    if existing_finalization_hint != finalization_hint {
        anyhow::bail!("finalization hint mismatch for circuit: {circuit_type}");
    }
    let existing_vk = keystore
        .load_recursive_layer_verification_key(circuit_type)
        .context("load_recursive_layer_verification_key()")?;
    if existing_vk.into_inner() != vk {
        anyhow::bail!("vk mismatch for circuit: {circuit_type}");
    }
    Ok(ProverSetupData {
        setup_base,
        setup,
        vk: vk.clone(),
        setup_tree,
        vars_hint,
        wits_hint,
        finalization_hint,
    })
}

#[cfg(not(feature = "gpu"))]
pub fn generate_gpu_setup_data(
    _keystore: &Keystore,
    _is_base_layer: bool,
    _numeric_circuit: u8,
    _dry_run: bool,
) -> anyhow::Result<String> {
    anyhow::bail!("Must compile with --gpu feature to use this option.")
}

#[cfg(feature = "gpu")]
pub fn generate_gpu_setup_data(
    keystore: &Keystore,
    is_base_layer: bool,
    numeric_circuit: u8,
    dry_run: bool,
) -> anyhow::Result<String> {
    let _context = ProverContext::create().context("failed initializing gpu prover context")?;
    let (cpu_setup_data, key) = match is_base_layer {
        true => {
            let circuit = get_base_layer_circuit(keystore, numeric_circuit)
                .context("get_base_layer_circuit()")?;
            (
                generate_cpu_base_layer_setup_data(keystore, circuit.clone())
                    .context("generate_cpu_base_layer_setup_data()")?,
                ProverServiceDataKey::new(numeric_circuit, AggregationRound::BasicCircuits),
            )
        }
        false => {
            let circuit = get_recursive_circuit(keystore, numeric_circuit)
                .context("get_recursive_circuit()")?;
            (
                generate_cpu_recursive_layer_setup_data(keystore, circuit.clone())
                    .context("generate_cpu_recursive_layer_setup_data()")?,
                ProverServiceDataKey::new_recursive(numeric_circuit),
            )
        }
    };
    let worker = Worker::new();
    let gpu_setup_data = GpuSetup::from_setup_and_hints(
        cpu_setup_data.setup_base,
        cpu_setup_data.setup_tree,
        cpu_setup_data.vars_hint.clone(),
        cpu_setup_data.wits_hint,
        &worker,
    )
    .context("failed creating GPU base layer setup data")?;
    let gpu_prover_setup_data = GpuProverSetupData {
        setup: gpu_setup_data,
        vk: cpu_setup_data.vk,
        finalization_hint: cpu_setup_data.finalization_hint,
    };
    // Serialization should always succeed.
    let serialized =
        bincode::serialize(&gpu_prover_setup_data).expect("Failed serializing setup data");
    let digest = md5::compute(&serialized);
    if !dry_run {
        keystore
            .save_setup_data_for_circuit_type(key, &serialized)
            .context("save_setup_data_for_circuit_type")?;
    } else {
        tracing::warn!("Dry run - not writing the key");
    }
    Ok(format!("{:?}", digest))
}
