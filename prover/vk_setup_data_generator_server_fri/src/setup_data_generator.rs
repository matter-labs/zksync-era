use std::collections::HashMap;

use anyhow::Context as _;
use circuit_definitions::{
    circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType,
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};
use zkevm_test_harness::{
    compute_setups::{generate_circuit_setup_data, CircuitSetupData},
    data_source::SetupDataSource,
};
use zksync_prover_fri_types::ProverServiceDataKey;
use zksync_types::basic_fri_types::AggregationRound;
use zksync_vk_setup_data_server_fri::{
    get_finalization_hints, get_round_for_recursive_circuit_type, load_keys_to_memory,
    save_setup_data, GoldilocksProverSetupData,
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

fn generate_setup_data_common(
    is_base_layer: bool,
    circuit_type: u8,
) -> anyhow::Result<CircuitSetupData> {
    let mut data_source = load_keys_to_memory()?;
    let circuit_setup_data =
        generate_circuit_setup_data(is_base_layer, circuit_type, &mut data_source).unwrap();

    let (finalization, vk) = if is_base_layer {
        (
            get_finalization_hints(ProverServiceDataKey::new(
                circuit_type,
                AggregationRound::BasicCircuits,
            ))?,
            data_source
                .get_base_layer_vk(circuit_type)
                .unwrap()
                .into_inner(),
        )
    } else {
        (
            get_finalization_hints(ProverServiceDataKey::new(
                circuit_type,
                get_round_for_recursive_circuit_type(circuit_type),
            ))?,
            data_source
                .get_recursion_layer_vk(circuit_type)
                .unwrap()
                .into_inner(),
        )
    };

    // Sanity check to make sure that generated setup data is matching.
    if finalization != circuit_setup_data.finalization_hint {
        anyhow::bail!("finalization hint mismatch for circuit: {circuit_type}");
    }
    if vk != circuit_setup_data.vk {
        anyhow::bail!("vk mismatch for circuit: {circuit_type}");
    }
    Ok(circuit_setup_data)
}

pub fn generate_cpu_setup_data(
    is_base_layer: bool,
    numeric_circuit: u8,
    dry_run: bool,
) -> anyhow::Result<String> {
    let circuit_setup_data = generate_setup_data_common(is_base_layer, numeric_circuit)?;

    let prover_setup_data = GoldilocksProverSetupData {
        setup_base: circuit_setup_data.setup_base,
        setup: circuit_setup_data.setup,
        vk: circuit_setup_data.vk,
        setup_tree: circuit_setup_data.setup_tree,
        vars_hint: circuit_setup_data.vars_hint,
        wits_hint: circuit_setup_data.wits_hint,
        finalization_hint: circuit_setup_data.finalization_hint,
    };
    let serialized = bincode::serialize(&prover_setup_data).expect("Failed serializing setup data");
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
    let circuit_setup_data = generate_setup_data_common(is_base_layer, numeric_circuit)?;

    let round = if is_base_layer {
        AggregationRound::BasicCircuits
    } else {
        get_round_for_recursive_circuit_type(numeric_circuit)
    };

    let worker = Worker::new();
    let gpu_setup_data = GpuSetup::from_setup_and_hints(
        circuit_setup_data.setup_base,
        circuit_setup_data.setup_tree,
        circuit_setup_data.vars_hint.clone(),
        circuit_setup_data.wits_hint,
        &worker,
    )
    .context("failed creating GPU base layer setup data")?;
    let gpu_prover_setup_data = GpuProverSetupData {
        setup: gpu_setup_data,
        vk: circuit_setup_data.vk,
        finalization_hint: circuit_setup_data.finalization_hint,
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
