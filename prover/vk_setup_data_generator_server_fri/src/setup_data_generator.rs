use std::collections::HashMap;

use anyhow::Context as _;
use circuit_definitions::{
    circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType,
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};
use zkevm_test_harness::{
    geometry_config::get_geometry_config, prover_utils::create_recursive_layer_setup_data,
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
use zksync_vk_setup_data_server_fri::{
    generate_cpu_base_layer_setup_data, get_finalization_hints,
    get_recursive_layer_vk_for_circuit_type, get_round_for_recursive_circuit_type, save_setup_data,
    utils::{
        get_basic_circuits, get_leaf_circuits, get_node_circuit, get_scheduler_circuit, CYCLE_LIMIT,
    },
    GoldilocksProverSetupData, ProverSetupData,
};
#[cfg(feature = "gpu")]
use {
    shivini::cs::setup::GpuSetup, shivini::ProverContext,
    zksync_vk_setup_data_server_fri::GpuProverSetupData,
};

fn generate_all(
    dry_run: bool,
    f: fn(bool, u8, bool) -> anyhow::Result<String>,
) -> anyhow::Result<HashMap<String, String>> {
    let mut result = HashMap::new();

    for numeric_circuit in
        BaseLayerCircuitType::VM as u8..=BaseLayerCircuitType::L1MessagesHasher as u8
    {
        let digest = f(true, numeric_circuit, dry_run)
            .context(format!("base layer, circuit {:?}", numeric_circuit))?;
        result.insert(format!("base_{}", numeric_circuit), digest);
    }

    // +2 - as '1' and '2' are scheduler and node respectively.
    for numeric_circuit in ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8
        ..=ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8
    {
        let digest = f(false, numeric_circuit, dry_run)
            .context(format!("recursive layer, circuit {:?}", numeric_circuit))?;
        result.insert(format!("recursive_{}", numeric_circuit), digest);
    }
    Ok(result)
}

pub fn generate_all_cpu_setup_data(dry_run: bool) -> anyhow::Result<HashMap<String, String>> {
    generate_all(dry_run, generate_cpu_setup_data)
}

pub fn generate_all_gpu_setup_data(dry_run: bool) -> anyhow::Result<HashMap<String, String>> {
    generate_all(dry_run, generate_gpu_setup_data)
}

pub fn generate_cpu_setup_data(
    is_base_layer: bool,
    numeric_circuit: u8,
    dry_run: bool,
) -> anyhow::Result<String> {
    match is_base_layer {
        true => {
            let circuit =
                get_base_layer_circuit(numeric_circuit).context("get_base_layer_circuit")?;
            let prover_setup_data = generate_cpu_base_layer_setup_data(circuit)
                .context("generate_cpu_base_layer_setup_data()")?;
            // Serialization should always succeed.
            let serialized =
                bincode::serialize(&prover_setup_data).expect("Failed serializing setup data");
            let digest = md5::compute(&serialized);

            if !dry_run {
                save_setup_data(
                    ProverServiceDataKey::new(numeric_circuit, AggregationRound::BasicCircuits),
                    &serialized,
                )
                .context("save_setup_data()")?;
            } else {
                tracing::warn!("Dry run - not writing the key");
            }
            Ok(format!("{:?}", digest))
        }
        false => {
            let circuit =
                get_recursive_circuit(numeric_circuit).context("get_recursive_circuit()")?;
            let prover_setup_data = generate_cpu_recursive_layer_setup_data(circuit)
                .context("generate_cpu_recursive_layer_setup_data()")?;
            // Serialization should always succeed.
            let serialized =
                bincode::serialize(&prover_setup_data).expect("Failed serializing setup data");
            let round = get_round_for_recursive_circuit_type(numeric_circuit);
            let digest = md5::compute(&serialized);
            if !dry_run {
                save_setup_data(
                    ProverServiceDataKey::new(numeric_circuit, round),
                    &serialized,
                )
                .context("save_setup_data()")?;
            } else {
                tracing::warn!("Dry run - not writing the key");
            }
            Ok(format!("{:?}", digest))
        }
    }
}

fn get_base_layer_circuit(
    id: u8,
) -> anyhow::Result<
    ZkSyncBaseLayerCircuit<
        GoldilocksField,
        VmWitnessOracle<GoldilocksField>,
        ZkSyncDefaultRoundFunction,
    >,
> {
    get_basic_circuits(CYCLE_LIMIT, get_geometry_config())
        .context("get_basic_circuits()")?
        .into_iter()
        .find(|circuit| id == circuit.numeric_circuit_type())
        .with_context(|| format!("No basic circuit found for id: {id}"))
}

fn get_recursive_circuit(id: u8) -> anyhow::Result<ZkSyncRecursiveLayerCircuit> {
    let mut recursive_circuits = get_leaf_circuits().context("get_leaf_circuits()")?;
    recursive_circuits.push(get_node_circuit().context("get_node_circuit()")?);
    recursive_circuits.push(get_scheduler_circuit().context("get_scheduler_circuit()")?);
    recursive_circuits
        .into_iter()
        .find(|circuit| id == circuit.numeric_circuit_type())
        .with_context(|| format!("No recursive circuit found for id: {id}"))
}

fn generate_cpu_recursive_layer_setup_data(
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
    let key = ProverServiceDataKey::new(
        circuit_type,
        get_round_for_recursive_circuit_type(circuit_type),
    );
    let existing_finalization_hint =
        get_finalization_hints(key).context("get_finalization_hints()")?;
    if existing_finalization_hint != finalization_hint {
        anyhow::bail!("finalization hint mismatch for circuit: {circuit_type}");
    }
    let existing_vk = get_recursive_layer_vk_for_circuit_type(circuit_type)
        .context("get_recursive_layer_vk_for_circuit_type()")?;
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
    _is_base_layer: bool,
    _numeric_circuit: u8,
    _dry_run: bool,
) -> anyhow::Result<String> {
    anyhow::bail!("Must compile with --gpu feature to use this option.")
}

#[cfg(feature = "gpu")]
pub fn generate_gpu_setup_data(
    is_base_layer: bool,
    numeric_circuit: u8,
    dry_run: bool,
) -> anyhow::Result<(String)> {
    let _context = ProverContext::create().context("failed initializing gpu prover context")?;
    let (cpu_setup_data, round) = match is_base_layer {
        true => {
            let circuit =
                get_base_layer_circuit(numeric_circuit).context("get_base_layer_circuit()")?;
            (
                generate_cpu_base_layer_setup_data(circuit.clone())
                    .context("generate_cpu_base_layer_setup_data()")?,
                AggregationRound::BasicCircuits,
            )
        }
        false => {
            let circuit =
                get_recursive_circuit(numeric_circuit).context("get_recursive_circuit()")?;
            (
                generate_cpu_recursive_layer_setup_data(circuit.clone())
                    .context("generate_cpu_recursive_layer_setup_data()")?,
                get_round_for_recursive_circuit_type(numeric_circuit),
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
        save_setup_data(
            ProverServiceDataKey::new(numeric_circuit, round),
            &serialized,
        )
        .context("save_setup_data")?;
    } else {
        tracing::warn!("Dry run - not writing the key");
    }
    Ok(format!("{:?}", digest))
}
