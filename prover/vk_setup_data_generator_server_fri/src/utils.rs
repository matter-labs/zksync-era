use crate::{
    get_base_layer_vk_for_circuit_type, get_base_path, get_recursive_layer_vk_for_circuit_type,
};
use zksync_prover_fri_types::circuit_definitions::aux_definitions::witness_oracle::VmWitnessOracle;
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::{GoldilocksExt2, GoldilocksField};
use zksync_prover_fri_types::circuit_definitions::boojum::gadgets::queue::full_state_queue::FullStateCircuitQueueRawWitness;
use zksync_prover_fri_types::circuit_definitions::boojum::gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge;
use zksync_prover_fri_types::circuit_definitions::boojum::gadgets::traits::allocatable::CSAllocatable;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::base_layer::ZkSyncBaseLayerCircuit;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::leaf_layer::ZkSyncLeafLayerRecursiveCircuit;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::node_layer::ZkSyncNodeLayerRecursiveCircuit;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::scheduler::SchedulerCircuit;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::{
    base_circuit_type_into_recursive_leaf_circuit_type, ZkSyncRecursionLayerStorageType,
    ZkSyncRecursionProof, ZkSyncRecursiveLayerCircuit, RECURSION_ARITY, SCHEDULER_CAPACITY,
};
use zksync_prover_fri_types::circuit_definitions::zk_evm::bytecode_to_code_hash;
use zksync_prover_fri_types::circuit_definitions::zk_evm::testing::storage::InMemoryStorage;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::recursion::leaf_layer::input::RecursionLeafParametersWitness;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::recursion::leaf_layer::input::{
    RecursionLeafInput, RecursionLeafInstanceWitness,
};
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::recursion::leaf_layer::LeafLayerRecursionConfig;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::recursion::node_layer::input::{
    RecursionNodeInput, RecursionNodeInstanceWitness,
};
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::recursion::node_layer::NodeLayerRecursionConfig;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::scheduler::aux::BaseLayerCircuitType;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::scheduler::input::SchedulerCircuitInstanceWitness;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::scheduler::SchedulerConfig;
use zksync_prover_fri_types::circuit_definitions::{
    base_layer_proof_config, recursion_layer_proof_config, zk_evm, ZkSyncDefaultRoundFunction,
};
use anyhow::Context as _;
use itertools::Itertools;
use std::collections::{HashMap, VecDeque};
use std::fs;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerProof;
use zkevm_test_harness::compute_setups::{
    generate_base_layer_vks_and_proofs, generate_recursive_layer_vks_and_proofs,
};
use zkevm_test_harness::data_source::BlockDataSource;
use zkevm_test_harness::ethereum_types::{Address, U256};
use zkevm_test_harness::external_calls::run;
use zkevm_test_harness::helper::artifact_utils::{save_predeployed_contracts, TestArtifact};
use zkevm_test_harness::sha3::{Digest, Keccak256};
use zkevm_test_harness::toolset::GeometryConfig;
use zkevm_test_harness::witness::full_block_artifact::{
    BlockBasicCircuits, BlockBasicCircuitsPublicCompactFormsWitnesses,
    BlockBasicCircuitsPublicInputs,
};
use zkevm_test_harness::witness::recursive_aggregation::compute_leaf_params;
use zkevm_test_harness::witness::tree::{BinarySparseStorageTree, ZKSyncTestingTree};

use zkevm_test_harness::data_source::in_memory_data_source::InMemoryDataSource;

pub const CYCLE_LIMIT: usize = 20000;

fn read_witness_artifact(filepath: &str) -> anyhow::Result<TestArtifact> {
    let text = fs::read_to_string(filepath)
        .with_context(|| format!("Failed to read witness artifact from path: {filepath}"))?;
    serde_json::from_str(text.as_str()).context("serde_json::from_str()")
}

pub fn get_basic_circuits(
    cycle_limit: usize,
    geometry: GeometryConfig,
) -> anyhow::Result<
    Vec<
        ZkSyncBaseLayerCircuit<
            GoldilocksField,
            VmWitnessOracle<GoldilocksField>,
            ZkSyncDefaultRoundFunction,
        >,
    >,
> {
    let path = format!("{}/witness_artifacts.json", get_base_path());
    let test_artifact = read_witness_artifact(&path).context("read_withess_artifact()")?;
    let (base_layer_circuit, _, _, _) = get_circuits(test_artifact, cycle_limit, geometry);
    Ok(base_layer_circuit
        .into_flattened_set()
        .into_iter()
        .dedup_by(|a, b| a.numeric_circuit_type() == b.numeric_circuit_type())
        .collect())
}

pub fn get_scheduler_proof_for_snark_vk_generation() -> anyhow::Result<ZkSyncRecursionLayerProof> {
    let path = format!("{}/scheduler_proof.bin", get_base_path());
    let proof_serialized = std::fs::read(path).context("Failed to read proof from file")?;
    bincode::deserialize::<ZkSyncRecursionLayerProof>(&proof_serialized)
        .context("Failed to deserialize proof")
}

pub fn get_leaf_circuits() -> anyhow::Result<Vec<ZkSyncRecursiveLayerCircuit>> {
    let mut circuits = vec![];
    for base_circuit_type in
        (BaseLayerCircuitType::VM as u8)..=(BaseLayerCircuitType::L1MessagesHasher as u8)
    {
        let input = RecursionLeafInput::placeholder_witness();
        let vk = get_base_layer_vk_for_circuit_type(base_circuit_type)
            .with_context(|| format!("get_base_layer_vk_for_circuit_type({base_circuit_type})"))?;

        let witness = RecursionLeafInstanceWitness {
            input,
            vk_witness: vk.clone().into_inner(),
            queue_witness: FullStateCircuitQueueRawWitness {
                elements: VecDeque::new(),
            },
            proof_witnesses: VecDeque::new(),
        };

        let config = LeafLayerRecursionConfig {
            proof_config: base_layer_proof_config(),
            vk_fixed_parameters: vk.into_inner().fixed_parameters,
            capacity: RECURSION_ARITY,
            _marker: std::marker::PhantomData,
        };

        let circuit = ZkSyncLeafLayerRecursiveCircuit {
            base_layer_circuit_type: BaseLayerCircuitType::from_numeric_value(base_circuit_type),
            witness,
            config,
            transcript_params: (),
            _marker: std::marker::PhantomData,
        };

        let circuit = ZkSyncRecursiveLayerCircuit::leaf_circuit_from_base_type(
            BaseLayerCircuitType::from_numeric_value(base_circuit_type),
            circuit,
        );
        circuits.push(circuit)
    }
    Ok(circuits)
}

pub fn get_node_circuit() -> anyhow::Result<ZkSyncRecursiveLayerCircuit> {
    let input = RecursionNodeInput::placeholder_witness();

    let input_vk = get_recursive_layer_vk_for_circuit_type(
        ZkSyncRecursionLayerStorageType::LeafLayerCircuitForMainVM as u8,
    )
    .context("get_recursive_layer_vk_for_circuit_type(LeafLAyerCircyutFromMainVM")?;
    let witness = RecursionNodeInstanceWitness {
        input,
        vk_witness: input_vk.clone().into_inner(),
        split_points: VecDeque::new(),
        proof_witnesses: VecDeque::new(),
    };

    let config = NodeLayerRecursionConfig {
        proof_config: recursion_layer_proof_config(),
        vk_fixed_parameters: input_vk.clone().into_inner().fixed_parameters,
        leaf_layer_capacity: RECURSION_ARITY,
        node_layer_capacity: RECURSION_ARITY,
        _marker: std::marker::PhantomData,
    };
    let circuit = ZkSyncNodeLayerRecursiveCircuit {
        witness,
        config,
        transcript_params: (),
        _marker: std::marker::PhantomData,
    };
    Ok(ZkSyncRecursiveLayerCircuit::NodeLayerCircuit(circuit))
}

pub fn get_scheduler_circuit() -> anyhow::Result<ZkSyncRecursiveLayerCircuit> {
    let mut scheduler_witness = SchedulerCircuitInstanceWitness::placeholder();

    // node VK
    let node_vk = get_recursive_layer_vk_for_circuit_type(
        ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
    )
    .context("get_recursive_layer_vk_for_circuit_type(NodeLayerCircuit)")?
    .into_inner();
    scheduler_witness.node_layer_vk_witness = node_vk.clone();

    let config = SchedulerConfig {
        proof_config: recursion_layer_proof_config(),
        vk_fixed_parameters: node_vk.fixed_parameters,
        capacity: SCHEDULER_CAPACITY,
        _marker: std::marker::PhantomData,
    };
    let scheduler_circuit = SchedulerCircuit {
        witness: scheduler_witness,
        config,
        transcript_params: (),
        _marker: std::marker::PhantomData,
    };
    Ok(ZkSyncRecursiveLayerCircuit::SchedulerCircuit(
        scheduler_circuit,
    ))
}

#[allow(dead_code)]
fn get_recursive_layer_proofs() -> Vec<ZkSyncRecursionProof> {
    let mut in_memory_source = InMemoryDataSource::new();
    generate_base_layer_vks_and_proofs(&mut in_memory_source).expect("Failed generating base vk's");
    generate_recursive_layer_vks_and_proofs(&mut in_memory_source)
        .expect("Failed generating recursive vk's");
    let mut scheduler_proofs: Vec<ZkSyncRecursionProof> = vec![];
    for recursive_circuit_type in (ZkSyncRecursionLayerStorageType::LeafLayerCircuitForMainVM as u8)
        ..=(ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8)
    {
        let proof = in_memory_source
            .get_node_layer_proof(recursive_circuit_type, 0, 0)
            .unwrap();
        scheduler_proofs.push(proof.into_inner());
    }
    scheduler_proofs
}

pub fn get_leaf_vk_params(
) -> anyhow::Result<Vec<(u8, RecursionLeafParametersWitness<GoldilocksField>)>> {
    let mut leaf_vk_commits = vec![];

    for circuit_type in
        (BaseLayerCircuitType::VM as u8)..=(BaseLayerCircuitType::L1MessagesHasher as u8)
    {
        let recursive_circuit_type = base_circuit_type_into_recursive_leaf_circuit_type(
            BaseLayerCircuitType::from_numeric_value(circuit_type),
        );
        let base_vk = get_base_layer_vk_for_circuit_type(circuit_type)
            .with_context(|| format!("get_base_layer_vk_for_circuit_type({circuit_type})"))?;
        let leaf_vk = get_recursive_layer_vk_for_circuit_type(recursive_circuit_type as u8)
            .with_context(|| {
                format!("get_recursive_layer_vk_for_circuit_type({recursive_circuit_type:?})")
            })?;
        let params = compute_leaf_params(circuit_type, base_vk, leaf_vk);
        leaf_vk_commits.push((circuit_type, params));
    }
    Ok(leaf_vk_commits)
}

#[allow(clippy::type_complexity)]
fn get_circuits(
    mut test_artifact: TestArtifact,
    cycle_limit: usize,
    geometry: GeometryConfig,
) -> (
    BlockBasicCircuits<GoldilocksField, ZkSyncDefaultRoundFunction>,
    BlockBasicCircuitsPublicInputs<GoldilocksField>,
    BlockBasicCircuitsPublicCompactFormsWitnesses<GoldilocksField>,
    SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
) {
    let round_function = ZkSyncDefaultRoundFunction::default();

    let mut storage_impl = InMemoryStorage::new();
    let mut tree = ZKSyncTestingTree::empty();

    test_artifact.entry_point_address =
        *zk_evm::zkevm_opcode_defs::system_params::BOOTLOADER_FORMAL_ADDRESS;

    let predeployed_contracts = test_artifact
        .predeployed_contracts
        .clone()
        .into_iter()
        .chain(Some((
            test_artifact.entry_point_address,
            test_artifact.entry_point_code.clone(),
        )))
        .collect::<HashMap<_, _>>();
    save_predeployed_contracts(&mut storage_impl, &mut tree, &predeployed_contracts);

    let used_bytecodes = HashMap::from_iter(
        test_artifact
            .predeployed_contracts
            .values()
            .map(|bytecode| {
                (
                    bytecode_to_code_hash(bytecode).unwrap().into(),
                    bytecode.clone(),
                )
            })
            .chain(
                Some(test_artifact.default_account_code.clone()).map(|bytecode| {
                    (
                        bytecode_to_code_hash(&bytecode).unwrap().into(),
                        bytecode.clone(),
                    )
                }),
            ),
    );

    let previous_enumeration_index = tree.next_enumeration_index();
    let previous_root = tree.root();
    // simualate content hash

    let mut hasher = Keccak256::new();
    hasher.update(previous_enumeration_index.to_be_bytes());
    hasher.update(previous_root);
    hasher.update(0u64.to_be_bytes()); // porter shard
    hasher.update([0u8; 32]); // porter shard

    let mut previous_data_hash = [0u8; 32];
    previous_data_hash[..].copy_from_slice(hasher.finalize().as_slice());

    let previous_aux_hash = [0u8; 32];
    let previous_meta_hash = [0u8; 32];

    let mut hasher = Keccak256::new();
    hasher.update(previous_data_hash);
    hasher.update(previous_meta_hash);
    hasher.update(previous_aux_hash);

    let mut previous_content_hash = [0u8; 32];
    previous_content_hash[..].copy_from_slice(hasher.finalize().as_slice());

    let default_account_codehash =
        bytecode_to_code_hash(&test_artifact.default_account_code).unwrap();
    let default_account_codehash = U256::from_big_endian(&default_account_codehash);

    let (
        basic_block_circuits,
        basic_block_circuits_inputs,
        closed_form_inputs,
        scheduler_partial_input,
        _,
    ) = run(
        Address::zero(),
        test_artifact.entry_point_address,
        test_artifact.entry_point_code,
        vec![],
        false,
        default_account_codehash,
        used_bytecodes,
        vec![],
        cycle_limit,
        round_function,
        geometry,
        storage_impl,
        &mut tree,
    );

    (
        basic_block_circuits,
        basic_block_circuits_inputs,
        closed_form_inputs,
        scheduler_partial_input,
    )
}
