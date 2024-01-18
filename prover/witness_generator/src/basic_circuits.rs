use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::Arc,
    time::Instant,
};

use anyhow::Context as _;
use async_trait::async_trait;
use multivm::vm_latest::{
    constants::MAX_CYCLES_FOR_TX, HistoryDisabled, StorageOracle as VmStorageOracle,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use zkevm_test_harness::{
    geometry_config::get_geometry_config,
    toolset::GeometryConfig,
    witness::full_block_artifact::{
        BlockBasicCircuits, BlockBasicCircuitsPublicCompactFormsWitnesses,
        BlockBasicCircuitsPublicInputs,
    },
};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_dal::{fri_witness_generator_dal::FriWitnessJobStatus, ConnectionPool};
use zksync_object_store::{
    Bucket, ClosedFormInputKey, ObjectStore, ObjectStoreFactory, StoredObject,
};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::{
            field::goldilocks::{GoldilocksExt2, GoldilocksField},
            gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
        },
        zkevm_circuits::scheduler::{
            block_header::BlockAuxilaryOutputWitness, input::SchedulerCircuitInstanceWitness,
        },
        ZkSyncDefaultRoundFunction,
    },
    get_current_pod_name, AuxOutputWitnessWrapper,
};
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_queued_job_processor::JobProcessor;
use zksync_state::{PostgresStorage, StorageView};
use zksync_types::{
    proofs::{AggregationRound, BasicCircuitWitnessGeneratorInput, PrepareBasicCircuitsJob},
    protocol_version::FriProtocolVersionId,
    Address, L1BatchNumber, ProtocolVersionId, BOOTLOADER_ADDRESS, H256, U256,
};
use zksync_utils::{bytes_to_chunks, h256_to_u256, u256_to_h256};

use crate::{
    metrics::WITNESS_GENERATOR_METRICS,
    precalculated_merkle_paths_provider::PrecalculatedMerklePathsProvider,
    storage_oracle::StorageOracle,
    utils::{
        expand_bootloader_contents, save_base_prover_input_artifacts, ClosedFormInputWrapper,
        SchedulerPartialInputWrapper,
    },
};

pub struct BasicCircuitArtifacts {
    basic_circuits: BlockBasicCircuits<GoldilocksField, ZkSyncDefaultRoundFunction>,
    basic_circuits_inputs: BlockBasicCircuitsPublicInputs<GoldilocksField>,
    per_circuit_closed_form_inputs: BlockBasicCircuitsPublicCompactFormsWitnesses<GoldilocksField>,
    #[allow(dead_code)]
    scheduler_witness: SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    aux_output_witness: BlockAuxilaryOutputWitness<GoldilocksField>,
}

#[derive(Debug)]
struct BlobUrls {
    circuit_ids_and_urls: Vec<(u8, String)>,
    closed_form_inputs_and_urls: Vec<(u8, String, usize)>,
    scheduler_witness_url: String,
}

#[derive(Clone)]
pub struct BasicWitnessGeneratorJob {
    block_number: L1BatchNumber,
    job: PrepareBasicCircuitsJob,
}

#[derive(Debug)]
pub struct BasicWitnessGenerator {
    config: Arc<FriWitnessGeneratorConfig>,
    object_store: Arc<dyn ObjectStore>,
    public_blob_store: Option<Arc<dyn ObjectStore>>,
    connection_pool: ConnectionPool,
    prover_connection_pool: ConnectionPool,
    protocol_versions: Vec<FriProtocolVersionId>,
}

impl BasicWitnessGenerator {
    pub async fn new(
        config: FriWitnessGeneratorConfig,
        store_factory: &ObjectStoreFactory,
        public_blob_store: Option<Arc<dyn ObjectStore>>,
        connection_pool: ConnectionPool,
        prover_connection_pool: ConnectionPool,
        protocol_versions: Vec<FriProtocolVersionId>,
    ) -> Self {
        Self {
            config: Arc::new(config),
            object_store: store_factory.create_store().await,
            public_blob_store,
            connection_pool,
            prover_connection_pool,
            protocol_versions,
        }
    }

    async fn process_job_impl(
        object_store: Arc<dyn ObjectStore>,
        connection_pool: ConnectionPool,
        prover_connection_pool: ConnectionPool,
        basic_job: BasicWitnessGeneratorJob,
        started_at: Instant,
        config: Arc<FriWitnessGeneratorConfig>,
    ) -> Option<BasicCircuitArtifacts> {
        let BasicWitnessGeneratorJob { block_number, job } = basic_job;
        let shall_force_process_block = config
            .force_process_block
            .map_or(false, |block| block == block_number.0);

        if let Some(blocks_proving_percentage) = config.blocks_proving_percentage {
            // Generate random number in (0; 100).
            let threshold = rand::thread_rng().gen_range(1..100);
            // We get value higher than `blocks_proving_percentage` with prob = `1 - blocks_proving_percentage`.
            // In this case job should be skipped.
            if threshold > blocks_proving_percentage && !shall_force_process_block {
                WITNESS_GENERATOR_METRICS.skipped_blocks.inc();
                tracing::info!(
                    "Skipping witness generation for block {}, blocks_proving_percentage: {}",
                    block_number.0,
                    blocks_proving_percentage
                );

                let mut prover_storage = prover_connection_pool.access_storage().await.unwrap();
                let mut transaction = prover_storage.start_transaction().await.unwrap();
                transaction
                    .fri_proof_compressor_dal()
                    .skip_proof_compression_job(block_number)
                    .await;
                transaction
                    .fri_witness_generator_dal()
                    .mark_witness_job(FriWitnessJobStatus::Skipped, block_number)
                    .await;
                transaction.commit().await.unwrap();
                return None;
            }
        }

        WITNESS_GENERATOR_METRICS.sampled_blocks.inc();
        tracing::info!(
            "Starting witness generation of type {:?} for block {}",
            AggregationRound::BasicCircuits,
            block_number.0
        );

        Some(
            process_basic_circuits_job(
                &*object_store,
                config,
                connection_pool,
                started_at,
                block_number,
                job,
            )
            .await,
        )
    }
}

#[async_trait]
impl JobProcessor for BasicWitnessGenerator {
    type Job = BasicWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    // The artifact is optional to support skipping blocks when sampling is enabled.
    type JobArtifacts = Option<BasicCircuitArtifacts>;

    const SERVICE_NAME: &'static str = "fri_basic_circuit_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.access_storage().await.unwrap();
        let last_l1_batch_to_process = self.config.last_l1_batch_to_process();
        let pod_name = get_current_pod_name();
        match prover_connection
            .fri_witness_generator_dal()
            .get_next_basic_circuit_witness_job(
                last_l1_batch_to_process,
                &self.protocol_versions,
                &pod_name,
            )
            .await
        {
            Some(block_number) => {
                tracing::info!(
                    "Processing FRI basic witness-gen for block {}",
                    block_number
                );
                let started_at = Instant::now();
                let job = get_artifacts(block_number, &*self.object_store).await;

                WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::BasicCircuits.into()]
                    .observe(started_at.elapsed());

                Ok(Some((block_number, job)))
            }
            None => Ok(None),
        }
    }

    async fn save_failure(&self, job_id: L1BatchNumber, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .access_storage()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_witness_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        job: BasicWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<Option<BasicCircuitArtifacts>>> {
        let config = Arc::clone(&self.config);
        let object_store = Arc::clone(&self.object_store);
        let connection_pool = self.connection_pool.clone();
        let prover_connection_pool = self.prover_connection_pool.clone();
        tokio::spawn(async move {
            Ok(Self::process_job_impl(
                object_store,
                connection_pool,
                prover_connection_pool,
                job,
                started_at,
                config,
            )
            .await)
        })
    }

    async fn save_result(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        optional_artifacts: Option<BasicCircuitArtifacts>,
    ) -> anyhow::Result<()> {
        match optional_artifacts {
            None => Ok(()),
            Some(artifacts) => {
                let blob_started_at = Instant::now();
                let blob_urls = save_artifacts(
                    job_id,
                    artifacts,
                    &*self.object_store,
                    self.public_blob_store.as_deref(),
                    self.config.shall_save_to_public_bucket,
                )
                .await;

                WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::BasicCircuits.into()]
                    .observe(blob_started_at.elapsed());

                update_database(&self.prover_connection_pool, started_at, job_id, blob_urls).await;
                Ok(())
            }
        }
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &L1BatchNumber) -> anyhow::Result<u32> {
        let mut prover_storage = self
            .prover_connection_pool
            .access_storage()
            .await
            .context("failed to acquire DB connection for BasicWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_basic_circuit_witness_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for BasicWitnessGenerator")
    }
}

async fn process_basic_circuits_job(
    object_store: &dyn ObjectStore,
    config: Arc<FriWitnessGeneratorConfig>,
    connection_pool: ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareBasicCircuitsJob,
) -> BasicCircuitArtifacts {
    let witness_gen_input =
        build_basic_circuits_witness_generator_input(&connection_pool, job, block_number).await;
    let (
        basic_circuits,
        basic_circuits_inputs,
        per_circuit_closed_form_inputs,
        scheduler_witness,
        aux_output_witness,
    ) = generate_witness(object_store, config, connection_pool, witness_gen_input).await;
    WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::BasicCircuits.into()]
        .observe(started_at.elapsed());

    tracing::info!(
        "Witness generation for block {} is complete in {:?}",
        block_number.0,
        started_at.elapsed()
    );

    BasicCircuitArtifacts {
        basic_circuits,
        basic_circuits_inputs,
        per_circuit_closed_form_inputs,
        scheduler_witness,
        aux_output_witness,
    }
}

async fn update_database(
    prover_connection_pool: &ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    blob_urls: BlobUrls,
) {
    let mut prover_connection = prover_connection_pool.access_storage().await.unwrap();
    let protocol_version_id = prover_connection
        .fri_witness_generator_dal()
        .protocol_version_for_l1_batch(block_number)
        .await;
    prover_connection
        .fri_prover_jobs_dal()
        .insert_prover_jobs(
            block_number,
            blob_urls.circuit_ids_and_urls,
            AggregationRound::BasicCircuits,
            0,
            protocol_version_id,
        )
        .await;
    prover_connection
        .fri_witness_generator_dal()
        .create_aggregation_jobs(
            block_number,
            &blob_urls.closed_form_inputs_and_urls,
            &blob_urls.scheduler_witness_url,
            get_recursive_layer_circuit_id_for_base_layer,
            protocol_version_id,
        )
        .await;
    prover_connection
        .fri_witness_generator_dal()
        .mark_witness_job_as_successful(block_number, started_at.elapsed())
        .await;
}

async fn get_artifacts(
    block_number: L1BatchNumber,
    object_store: &dyn ObjectStore,
) -> BasicWitnessGeneratorJob {
    let job = object_store.get(block_number).await.unwrap();
    BasicWitnessGeneratorJob { block_number, job }
}

async fn save_artifacts(
    block_number: L1BatchNumber,
    artifacts: BasicCircuitArtifacts,
    object_store: &dyn ObjectStore,
    public_object_store: Option<&dyn ObjectStore>,
    shall_save_to_public_bucket: bool,
) -> BlobUrls {
    let circuit_ids_and_urls = save_base_prover_input_artifacts(
        block_number,
        artifacts.basic_circuits,
        object_store,
        AggregationRound::BasicCircuits,
    )
    .await;
    let closed_form_inputs_and_urls = save_leaf_aggregation_artifacts(
        block_number,
        artifacts.basic_circuits_inputs,
        artifacts.per_circuit_closed_form_inputs,
        object_store,
    )
    .await;
    let scheduler_witness_url = save_scheduler_artifacts(
        block_number,
        artifacts.scheduler_witness,
        artifacts.aux_output_witness,
        object_store,
        public_object_store,
        shall_save_to_public_bucket,
    )
    .await;

    BlobUrls {
        circuit_ids_and_urls,
        closed_form_inputs_and_urls,
        scheduler_witness_url,
    }
}

async fn save_scheduler_artifacts(
    block_number: L1BatchNumber,
    scheduler_partial_input: SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    aux_output_witness: BlockAuxilaryOutputWitness<GoldilocksField>,
    object_store: &dyn ObjectStore,
    public_object_store: Option<&dyn ObjectStore>,
    shall_save_to_public_bucket: bool,
) -> String {
    let aux_output_witness_wrapper = AuxOutputWitnessWrapper(aux_output_witness);
    if shall_save_to_public_bucket {
        public_object_store
            .expect("public_object_store shall not be empty while running with shall_save_to_public_bucket config")
            .put(block_number, &aux_output_witness_wrapper)
            .await
            .unwrap();
    }
    object_store
        .put(block_number, &aux_output_witness_wrapper)
        .await
        .unwrap();
    let wrapper = SchedulerPartialInputWrapper(scheduler_partial_input);
    object_store.put(block_number, &wrapper).await.unwrap()
}

async fn save_leaf_aggregation_artifacts(
    block_number: L1BatchNumber,
    basic_circuits_inputs: BlockBasicCircuitsPublicInputs<GoldilocksField>,
    per_circuit_closed_form_inputs: BlockBasicCircuitsPublicCompactFormsWitnesses<GoldilocksField>,
    object_store: &dyn ObjectStore,
) -> Vec<(u8, String, usize)> {
    let round_function = ZkSyncDefaultRoundFunction::default();
    let queues = basic_circuits_inputs
        .into_recursion_queues(per_circuit_closed_form_inputs, &round_function);
    let mut circuit_id_urls_with_count = Vec::with_capacity(queues.len());
    for (circuit_id_ref, recursion_queue_simulator, inputs) in queues {
        let circuit_id = circuit_id_ref as u8;
        let key = ClosedFormInputKey {
            block_number,
            circuit_id,
        };
        let basic_circuit_count = inputs.len();
        let wrapper = ClosedFormInputWrapper(inputs, recursion_queue_simulator);
        let blob_url = object_store.put(key, &wrapper).await.unwrap();
        circuit_id_urls_with_count.push((circuit_id, blob_url, basic_circuit_count))
    }
    circuit_id_urls_with_count
}

// If making changes to this method, consider moving this logic to the DAL layer and make
// `PrepareBasicCircuitsJob` have all fields of `BasicCircuitWitnessGeneratorInput`.
async fn build_basic_circuits_witness_generator_input(
    connection_pool: &ConnectionPool,
    witness_merkle_input: PrepareBasicCircuitsJob,
    l1_batch_number: L1BatchNumber,
) -> BasicCircuitWitnessGeneratorInput {
    let mut connection = connection_pool.access_storage().await.unwrap();
    let block_header = connection
        .blocks_dal()
        .get_l1_batch_header(l1_batch_number)
        .await
        .unwrap()
        .unwrap();
    let initial_heap_content = connection
        .blocks_dal()
        .get_initial_bootloader_heap(l1_batch_number)
        .await
        .unwrap()
        .unwrap();
    let (_, previous_block_timestamp) = connection
        .blocks_dal()
        .get_l1_batch_state_root_and_timestamp(l1_batch_number - 1)
        .await
        .unwrap()
        .unwrap();
    let previous_block_hash = connection
        .blocks_dal()
        .get_l1_batch_state_root(l1_batch_number - 1)
        .await
        .unwrap()
        .expect("cannot generate witness before the root hash is computed");
    BasicCircuitWitnessGeneratorInput {
        block_number: l1_batch_number,
        previous_block_timestamp,
        previous_block_hash,
        block_timestamp: block_header.timestamp,
        used_bytecodes_hashes: block_header.used_contract_hashes,
        initial_heap_content,
        merkle_paths_input: witness_merkle_input,
    }
}

async fn generate_witness(
    object_store: &dyn ObjectStore,
    config: Arc<FriWitnessGeneratorConfig>,
    connection_pool: ConnectionPool,
    input: BasicCircuitWitnessGeneratorInput,
) -> (
    BlockBasicCircuits<GoldilocksField, ZkSyncDefaultRoundFunction>,
    BlockBasicCircuitsPublicInputs<GoldilocksField>,
    BlockBasicCircuitsPublicCompactFormsWitnesses<GoldilocksField>,
    SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    BlockAuxilaryOutputWitness<GoldilocksField>,
) {
    let mut connection = connection_pool.access_storage().await.unwrap();
    let header = connection
        .blocks_dal()
        .get_l1_batch_header(input.block_number)
        .await
        .unwrap()
        .unwrap();

    let protocol_version = header
        .protocol_version
        .unwrap_or(ProtocolVersionId::last_potentially_undefined());

    let previous_batch_with_metadata = connection
        .blocks_dal()
        .get_l1_batch_metadata(zksync_types::L1BatchNumber(
            input.block_number.checked_sub(1).unwrap(),
        ))
        .await
        .unwrap()
        .unwrap();

    let bootloader_code_bytes = connection
        .storage_dal()
        .get_factory_dep(header.base_system_contracts_hashes.bootloader)
        .await
        .expect("Bootloader bytecode should exist");
    let bootloader_code = bytes_to_chunks(&bootloader_code_bytes);
    let account_bytecode_bytes = connection
        .storage_dal()
        .get_factory_dep(header.base_system_contracts_hashes.default_aa)
        .await
        .expect("Default aa bytecode should exist");
    let account_bytecode = bytes_to_chunks(&account_bytecode_bytes);
    let bootloader_contents =
        expand_bootloader_contents(&input.initial_heap_content, protocol_version);
    let account_code_hash = h256_to_u256(header.base_system_contracts_hashes.default_aa);

    let hashes: HashSet<H256> = input
        .used_bytecodes_hashes
        .iter()
        // SMA-1555: remove this hack once updated to the latest version of `zkevm_test_harness`
        .filter(|&&hash| hash != h256_to_u256(header.base_system_contracts_hashes.bootloader))
        .map(|hash| u256_to_h256(*hash))
        .collect();

    let storage_refunds = connection
        .blocks_dal()
        .get_storage_refunds(input.block_number)
        .await
        .unwrap()
        .unwrap();

    let mut used_bytecodes = connection.storage_dal().get_factory_deps(&hashes).await;
    if input.used_bytecodes_hashes.contains(&account_code_hash) {
        used_bytecodes.insert(account_code_hash, account_bytecode);
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
        .await
        .unwrap()
        .expect("L1 batch should contain at least one miniblock");
    drop(connection);

    let mut tree = PrecalculatedMerklePathsProvider::new(
        input.merkle_paths_input,
        input.previous_block_hash.0,
    );
    let geometry_config = get_geometry_config();
    let mut hasher = DefaultHasher::new();
    geometry_config.hash(&mut hasher);
    tracing::info!(
        "generating witness for block {} using geometry config hash: {}",
        input.block_number.0,
        hasher.finish()
    );

    let should_dump_arguments = config
        .dump_arguments_for_blocks
        .contains(&input.block_number.0);
    if should_dump_arguments {
        save_run_with_fixed_params_args_to_gcs(
            object_store,
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
            geometry_config,
            tree.clone(),
        )
        .await;
    }

    // The following part is CPU-heavy, so we move it to a separate thread.
    let rt_handle = tokio::runtime::Handle::current();

    let (
        basic_circuits,
        basic_circuits_public_inputs,
        basic_circuits_public_compact_witness,
        mut scheduler_witness,
        block_aux_witness,
    ) = tokio::task::spawn_blocking(move || {
        let connection = rt_handle
            .block_on(connection_pool.access_storage())
            .unwrap();

        let storage = PostgresStorage::new(rt_handle, connection, last_miniblock_number, true);
        let storage_view = StorageView::new(storage).to_rc_ptr();

        let vm_storage_oracle: VmStorageOracle<StorageView<PostgresStorage<'_>>, HistoryDisabled> =
            VmStorageOracle::new(storage_view.clone());
        let storage_oracle = StorageOracle::new(vm_storage_oracle, storage_refunds);

        zkevm_test_harness::external_calls::run_with_fixed_params(
            Address::zero(),
            BOOTLOADER_ADDRESS,
            bootloader_code,
            bootloader_contents,
            false,
            account_code_hash,
            used_bytecodes,
            Vec::default(),
            MAX_CYCLES_FOR_TX as usize,
            geometry_config,
            storage_oracle,
            &mut tree,
        )
    })
    .await
    .unwrap();

    scheduler_witness.previous_block_meta_hash =
        previous_batch_with_metadata.metadata.meta_parameters_hash.0;
    scheduler_witness.previous_block_aux_hash =
        previous_batch_with_metadata.metadata.aux_data_hash.0;

    (
        basic_circuits,
        basic_circuits_public_inputs,
        basic_circuits_public_compact_witness,
        scheduler_witness,
        block_aux_witness,
    )
}

#[allow(clippy::too_many_arguments)]
async fn save_run_with_fixed_params_args_to_gcs(
    object_store: &dyn ObjectStore,
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
    object_store
        .put(L1BatchNumber(l1_batch_number), &run_with_fixed_params_input)
        .await
        .unwrap();
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RunWithFixedParamsInput {
    pub l1_batch_number: u32,
    pub last_miniblock_number: u32,
    pub caller: Address,
    pub entry_point_address: Address,
    pub entry_point_code: Vec<[u8; 32]>,
    pub initial_heap_content: Vec<u8>,
    pub zk_porter_is_available: bool,
    pub default_aa_code_hash: U256,
    pub used_bytecodes: HashMap<U256, Vec<[u8; 32]>>,
    pub ram_verification_queries: Vec<(u32, U256)>,
    pub cycle_limit: usize,
    pub geometry: GeometryConfig,
    pub tree: PrecalculatedMerklePathsProvider,
}

impl StoredObject for RunWithFixedParamsInput {
    const BUCKET: Bucket = Bucket::WitnessInput;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("run_with_fixed_params_input_{}.bin", key)
    }

    zksync_object_store::serialize_using_bincode!();
}
