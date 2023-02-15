use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::Instant;
use vm::zk_evm::bitflags::_core::cell::RefCell;
use vm::zk_evm::ethereum_types::H256;
use vm::{StorageOracle, MAX_CYCLES_FOR_TX};
use zksync_config::configs::WitnessGeneratorConfig;
use zksync_config::constants::BOOTLOADER_ADDRESS;
use zksync_contracts::{read_proved_block_bootloader_bytecode, read_sys_contract_bytecode};
use zksync_dal::ConnectionPool;
use zksync_object_store::gcs_utils::{
    basic_circuits_blob_url, basic_circuits_inputs_blob_url, merkle_tree_paths_blob_url,
    scheduler_witness_blob_url,
};
use zksync_object_store::object_store::create_object_store_from_env;
use zksync_object_store::object_store::{
    DynamicObjectStore, LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
    SCHEDULER_WITNESS_JOBS_BUCKET_PATH, WITNESS_INPUT_BUCKET_PATH,
};
use zksync_state::storage_view::StorageView;
use zksync_types::zkevm_test_harness::toolset::GeometryConfig;
use zksync_types::{
    circuit::GEOMETRY_CONFIG,
    proofs::{
        AggregationRound, BasicCircuitWitnessGeneratorInput, PrepareBasicCircuitsJob,
        StorageLogMetadata, WitnessGeneratorJob, WitnessGeneratorJobInput,
    },
    zkevm_test_harness::{
        bellman::bn256::Bn256,
        witness::full_block_artifact::{BlockBasicCircuits, BlockBasicCircuitsPublicInputs},
        SchedulerCircuitInstanceWitness,
    },
    Address, L1BatchNumber, U256,
};
use zksync_utils::bytecode::hash_bytecode;
use zksync_utils::{bytes_to_chunks, h256_to_u256, u256_to_h256};

use crate::db_storage_provider::DbStorageProvider;
use crate::witness_generator;
use crate::witness_generator::precalculated_merkle_paths_provider::PrecalculatedMerklePathsProvider;
use crate::witness_generator::track_witness_generation_stage;
use crate::witness_generator::utils::{expand_bootloader_contents, save_prover_input_artifacts};

pub struct BasicCircuitArtifacts {
    pub basic_circuits: BlockBasicCircuits<Bn256>,
    pub basic_circuits_inputs: BlockBasicCircuitsPublicInputs<Bn256>,
    pub scheduler_witness: SchedulerCircuitInstanceWitness<Bn256>,
    pub serialized_circuits: Vec<(String, Vec<u8>)>,
}

pub fn process_basic_circuits_job(
    config: WitnessGeneratorConfig,
    connection_pool: ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareBasicCircuitsJob,
) -> BasicCircuitArtifacts {
    let witness_gen_input =
        build_basic_circuits_witness_generator_input(connection_pool.clone(), job, block_number);

    let (basic_circuits, basic_circuits_inputs, scheduler_witness) =
        generate_witness(config, connection_pool, witness_gen_input);

    let individual_circuits = basic_circuits.clone().into_flattened_set();

    let serialized_circuits: Vec<(String, Vec<u8>)> =
        witness_generator::serialize_circuits(&individual_circuits);

    let total_size_bytes: usize = serialized_circuits
        .iter()
        .map(|(_, bytes)| bytes.len())
        .sum();
    vlog::info!(
        "Witness generation for block {} is complete in {:?}. Number of circuits: {}, total size: {}KB",
        block_number.0,
        started_at.elapsed(),
        serialized_circuits.len(),
        total_size_bytes >> 10
    );

    BasicCircuitArtifacts {
        basic_circuits,
        basic_circuits_inputs,
        scheduler_witness,
        serialized_circuits,
    }
}

pub fn update_database(
    connection_pool: ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    circuits: Vec<String>,
) {
    let mut connection = connection_pool.access_storage_blocking();
    let mut transaction = connection.start_transaction_blocking();

    transaction
        .witness_generator_dal()
        .create_aggregation_jobs(block_number, circuits.len());
    transaction.prover_dal().insert_prover_jobs(
        block_number,
        circuits,
        AggregationRound::BasicCircuits,
    );
    transaction
        .witness_generator_dal()
        .mark_witness_job_as_successful(
            block_number,
            AggregationRound::BasicCircuits,
            started_at.elapsed(),
        );

    transaction.commit_blocking();
    track_witness_generation_stage(block_number, started_at, AggregationRound::BasicCircuits);
}

pub async fn get_artifacts(
    block_number: L1BatchNumber,
    object_store: &DynamicObjectStore,
) -> WitnessGeneratorJob {
    let merkle_tree_paths = object_store
        .get(
            WITNESS_INPUT_BUCKET_PATH,
            merkle_tree_paths_blob_url(block_number),
        )
        .unwrap();

    let (merkle_paths, next_enumeration_index) =
        bincode::deserialize::<(Vec<StorageLogMetadata>, u64)>(&merkle_tree_paths)
            .expect("witness deserialization failed");

    WitnessGeneratorJob {
        block_number,
        job: WitnessGeneratorJobInput::BasicCircuits(Box::new(PrepareBasicCircuitsJob {
            merkle_paths,
            next_enumeration_index,
        })),
    }
}

pub async fn save_artifacts(
    block_number: L1BatchNumber,
    artifacts: BasicCircuitArtifacts,
    object_store: &mut DynamicObjectStore,
) {
    let basic_circuits_serialized =
        bincode::serialize(&artifacts.basic_circuits).expect("cannot serialize basic_circuits");
    object_store
        .put(
            LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
            basic_circuits_blob_url(block_number),
            basic_circuits_serialized,
        )
        .unwrap();

    let basic_circuits_inputs_serialized = bincode::serialize(&artifacts.basic_circuits_inputs)
        .expect("cannot serialize basic_circuits_inputs");
    object_store
        .put(
            LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
            basic_circuits_inputs_blob_url(block_number),
            basic_circuits_inputs_serialized,
        )
        .unwrap();

    let scheduler_witness_serialized = bincode::serialize(&artifacts.scheduler_witness)
        .expect("cannot serialize scheduler_witness");

    object_store
        .put(
            SCHEDULER_WITNESS_JOBS_BUCKET_PATH,
            scheduler_witness_blob_url(block_number),
            scheduler_witness_serialized,
        )
        .unwrap();
    save_prover_input_artifacts(
        block_number,
        artifacts.serialized_circuits,
        object_store,
        AggregationRound::BasicCircuits,
    )
    .await;
}

// If making changes to this method, consider moving this logic to the DAL layer and make
// `PrepareBasicCircuitsJob` have all fields of `BasicCircuitWitnessGeneratorInput`.
pub fn build_basic_circuits_witness_generator_input(
    connection_pool: ConnectionPool,
    witness_merkle_input: PrepareBasicCircuitsJob,
    block_number: L1BatchNumber,
) -> BasicCircuitWitnessGeneratorInput {
    let mut connection = connection_pool.access_storage_blocking();
    let block_header = connection
        .blocks_dal()
        .get_block_header(block_number)
        .unwrap();
    let previous_block_header = connection
        .blocks_dal()
        .get_block_header(block_number - 1)
        .unwrap();
    BasicCircuitWitnessGeneratorInput {
        block_number,
        previous_block_timestamp: previous_block_header.timestamp,
        block_timestamp: block_header.timestamp,
        used_bytecodes_hashes: block_header.used_contract_hashes,
        initial_heap_content: block_header.initial_bootloader_contents,
        merkle_paths_input: witness_merkle_input,
    }
}

pub fn generate_witness(
    config: WitnessGeneratorConfig,
    connection_pool: ConnectionPool,
    input: BasicCircuitWitnessGeneratorInput,
) -> (
    BlockBasicCircuits<Bn256>,
    BlockBasicCircuitsPublicInputs<Bn256>,
    SchedulerCircuitInstanceWitness<Bn256>,
) {
    let mut connection = connection_pool.access_storage_blocking();

    let account_bytecode = read_sys_contract_bytecode("", "DefaultAccount");
    let account_code_hash = h256_to_u256(hash_bytecode(&account_bytecode));
    let bootloader_code_bytes = read_proved_block_bootloader_bytecode();
    let bootloader_code_hash = h256_to_u256(hash_bytecode(&bootloader_code_bytes));
    let bootloader_code = bytes_to_chunks(&bootloader_code_bytes);
    let bootloader_contents = expand_bootloader_contents(input.initial_heap_content);

    let hashes: HashSet<H256> = input
        .used_bytecodes_hashes
        .iter()
        // SMA-1555: remove this hack once updated to the latest version of zkevm_test_harness
        .filter(|&&hash| hash != bootloader_code_hash)
        .map(|hash| u256_to_h256(*hash))
        .collect();

    let mut used_bytecodes = connection.storage_dal().get_factory_deps(&hashes);
    if input.used_bytecodes_hashes.contains(&account_code_hash) {
        used_bytecodes.insert(account_code_hash, bytes_to_chunks(&account_bytecode));
    }

    assert_eq!(
        hashes.len(),
        used_bytecodes.len(),
        "{} factory deps are not found in DB",
        hashes.len() - used_bytecodes.len()
    );

    // `DbStorageProvider` was designed to be used in API, so it accepts miniblock numbers.
    // Probably, we should make it work with L1 batch numbers too.
    let (_, last_miniblock_number) = connection
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(input.block_number - 1)
        .expect("L1 batch should contain at least one miniblock");
    let db_storage_provider = DbStorageProvider::new(connection, last_miniblock_number, true);
    let mut tree = PrecalculatedMerklePathsProvider::new(input.merkle_paths_input);

    let storage_ptr: &mut dyn vm::storage::Storage = &mut StorageView::new(db_storage_provider);
    let storage_oracle = StorageOracle::new(Rc::new(RefCell::new(storage_ptr)));
    let mut hasher = DefaultHasher::new();
    GEOMETRY_CONFIG.hash(&mut hasher);
    vlog::info!(
        "generating witness for block {} using geometry config hash: {}",
        input.block_number.0,
        hasher.finish()
    );
    if config
        .dump_arguments_for_blocks
        .contains(&input.block_number.0)
    {
        save_run_with_fixed_params_args_to_gcs(
            input.block_number.0,
            last_miniblock_number.0,
            Address::zero(),
            BOOTLOADER_ADDRESS,
            bootloader_code.clone(),
            bootloader_contents.clone(),
            false,
            account_code_hash,
            used_bytecodes.clone(),
            Vec::default(),
            MAX_CYCLES_FOR_TX as usize,
            GEOMETRY_CONFIG,
            tree.clone(),
        );
    }

    zksync_types::zkevm_test_harness::external_calls::run_with_fixed_params(
        Address::zero(),
        BOOTLOADER_ADDRESS,
        bootloader_code,
        bootloader_contents,
        false,
        account_code_hash,
        used_bytecodes,
        Vec::default(),
        MAX_CYCLES_FOR_TX as usize,
        GEOMETRY_CONFIG,
        storage_oracle,
        &mut tree,
    )
}

#[allow(clippy::too_many_arguments)]
fn save_run_with_fixed_params_args_to_gcs(
    l1_batch_number: u32,
    last_miniblock_number: u32,
    caller: Address,
    entry_point_address: Address,
    entry_point_code: Vec<[u8; 32]>,
    initial_heap_content: Vec<u8>,
    zk_porter_is_available: bool,
    default_aa_code_hash: U256,
    used_bytecodes: HashMap<U256, Vec<[u8; 32]>>,
    ram_verification_queries: Vec<(u32, U256)>,
    cycle_limit: usize,
    geometry: GeometryConfig,
    tree: PrecalculatedMerklePathsProvider,
) {
    let run_with_fixed_params_input = RunWithFixedParamsInput {
        l1_batch_number,
        last_miniblock_number,
        caller,
        entry_point_address,
        entry_point_code,
        initial_heap_content,
        zk_porter_is_available,
        default_aa_code_hash,
        used_bytecodes,
        ram_verification_queries,
        cycle_limit,
        geometry,
        tree,
    };
    let run_with_fixed_params_input_serialized = bincode::serialize(&run_with_fixed_params_input)
        .expect("cannot serialize run_with_fixed_params_input");
    let blob_url = format!("run_with_fixed_params_input_{}.bin", l1_batch_number);
    let mut object_store = create_object_store_from_env();
    object_store
        .put(
            WITNESS_INPUT_BUCKET_PATH,
            blob_url,
            run_with_fixed_params_input_serialized,
        )
        .unwrap();
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RunWithFixedParamsInput {
    l1_batch_number: u32,
    last_miniblock_number: u32,
    caller: Address,
    entry_point_address: Address,
    entry_point_code: Vec<[u8; 32]>,
    initial_heap_content: Vec<u8>,
    zk_porter_is_available: bool,
    default_aa_code_hash: U256,
    used_bytecodes: HashMap<U256, Vec<[u8; 32]>>,
    ram_verification_queries: Vec<(u32, U256)>,
    cycle_limit: usize,
    geometry: GeometryConfig,
    tree: PrecalculatedMerklePathsProvider,
}
