use async_trait::async_trait;
use rand::Rng;
use serde::{Deserialize, Serialize};

use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::Arc,
    time::Instant,
};

use multivm::vm_latest::{
    constants::MAX_CYCLES_FOR_TX, HistoryDisabled, SimpleMemory, StorageOracle as VmStorageOracle,
};
use zksync_config::configs::{
    witness_generator::BasicWitnessGeneratorDataSource, WitnessGeneratorConfig,
};
use zksync_dal::ConnectionPool;
use zksync_object_store::{Bucket, ObjectStore, ObjectStoreFactory, StoredObject};
use zksync_queued_job_processor::JobProcessor;
use zksync_state::{PostgresStorage, ReadStorage, ShadowStorage, StorageView, WitnessStorage};
use zksync_system_constants::BOOTLOADER_ADDRESS;
use zksync_types::{
    circuit::GEOMETRY_CONFIG,
    proofs::{AggregationRound, BasicCircuitWitnessGeneratorInput, PrepareBasicCircuitsJob},
    zkevm_test_harness::toolset::GeometryConfig,
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit,
        bellman::bn256::Bn256,
        witness::full_block_artifact::{BlockBasicCircuits, BlockBasicCircuitsPublicInputs},
        witness::oracle::VmWitnessOracle,
        SchedulerCircuitInstanceWitness,
    },
    Address, L1BatchNumber, ProtocolVersionId, H256, U256, USED_BOOTLOADER_MEMORY_BYTES,
};
use zksync_utils::{bytes_to_chunks, expand_memory_contents, h256_to_u256, u256_to_h256};

use super::{
    precalculated_merkle_paths_provider::PrecalculatedMerklePathsProvider,
    storage_oracle::StorageOracle, utils::save_prover_input_artifacts, METRICS,
};

pub struct BasicCircuitArtifacts {
    basic_circuits: BlockBasicCircuits<Bn256>,
    basic_circuits_inputs: BlockBasicCircuitsPublicInputs<Bn256>,
    scheduler_witness: SchedulerCircuitInstanceWitness<Bn256>,
    circuits: Vec<ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
}

#[derive(Debug)]
struct BlobUrls {
    basic_circuits_url: String,
    basic_circuits_inputs_url: String,
    scheduler_witness_url: String,
    circuit_types_and_urls: Vec<(&'static str, String)>,
}

#[derive(Clone)]
pub struct BasicWitnessGeneratorJob {
    block_number: L1BatchNumber,
    job: PrepareBasicCircuitsJob,
}

#[derive(Debug)]
pub struct BasicWitnessGenerator {
    config: WitnessGeneratorConfig,
    object_store: Arc<dyn ObjectStore>,
    protocol_versions: Vec<ProtocolVersionId>,
    connection_pool: ConnectionPool,
    prover_connection_pool: ConnectionPool,
}

impl BasicWitnessGenerator {
    pub async fn new(
        config: WitnessGeneratorConfig,
        store_factory: &ObjectStoreFactory,
        protocol_versions: Vec<ProtocolVersionId>,
        connection_pool: ConnectionPool,
        prover_connection_pool: ConnectionPool,
    ) -> Self {
        Self {
            config,
            object_store: store_factory.create_store().await.into(),
            protocol_versions,
            connection_pool,
            prover_connection_pool,
        }
    }

    async fn process_job_impl(
        config: WitnessGeneratorConfig,
        object_store: Arc<dyn ObjectStore>,
        connection_pool: ConnectionPool,
        prover_connection_pool: ConnectionPool,
        basic_job: BasicWitnessGeneratorJob,
        started_at: Instant,
    ) -> anyhow::Result<Option<BasicCircuitArtifacts>> {
        let BasicWitnessGeneratorJob { block_number, job } = basic_job;

        if let Some(blocks_proving_percentage) = config.blocks_proving_percentage {
            // Generate random number in (0; 100).
            let threshold = rand::thread_rng().gen_range(1..100);
            // We get value higher than `blocks_proving_percentage` with prob = `1 - blocks_proving_percentage`.
            // In this case job should be skipped.
            if threshold > blocks_proving_percentage {
                METRICS.skipped_blocks.inc();
                tracing::info!(
                    "Skipping witness generation for block {}, blocks_proving_percentage: {}",
                    block_number.0,
                    blocks_proving_percentage
                );
                let mut storage = connection_pool.access_storage().await.unwrap();
                storage
                    .blocks_dal()
                    .set_skip_proof_for_l1_batch(block_number)
                    .await
                    .unwrap();
                let mut prover_storage = prover_connection_pool.access_storage().await.unwrap();
                prover_storage
                    .witness_generator_dal()
                    .mark_witness_job_as_skipped(block_number, AggregationRound::BasicCircuits)
                    .await;
                return Ok(None);
            }
        }

        METRICS.sampled_blocks.inc();
        tracing::info!(
            "Starting witness generation of type {:?} for block {}",
            AggregationRound::BasicCircuits,
            block_number.0
        );

        Ok(Some(
            process_basic_circuits_job(
                object_store,
                config,
                connection_pool,
                started_at,
                block_number,
                job,
            )
            .await,
        ))
    }
}

#[async_trait]
impl JobProcessor for BasicWitnessGenerator {
    type Job = BasicWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    // The artifact is optional to support skipping blocks when sampling is enabled.
    type JobArtifacts = Option<BasicCircuitArtifacts>;

    const SERVICE_NAME: &'static str = "basic_circuit_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.access_storage().await.unwrap();
        let last_l1_batch_to_process = self.config.last_l1_batch_to_process();

        Ok(
            match prover_connection
                .witness_generator_dal()
                .get_next_basic_circuit_witness_job(
                    self.config.witness_generation_timeout(),
                    self.config.max_attempts,
                    last_l1_batch_to_process,
                    &self.protocol_versions,
                )
                .await
            {
                Some(metadata) => {
                    let job = get_artifacts(metadata.block_number, &self.object_store).await;
                    Some((job.block_number, job))
                }
                None => None,
            },
        )
    }

    async fn save_failure(&self, job_id: L1BatchNumber, started_at: Instant, error: String) -> () {
        let attempts = self
            .prover_connection_pool
            .access_storage()
            .await
            .unwrap()
            .witness_generator_dal()
            .mark_witness_job_as_failed(
                AggregationRound::BasicCircuits,
                job_id,
                started_at.elapsed(),
                error,
            )
            .await;

        if attempts >= self.config.max_attempts {
            self.connection_pool
                .access_storage()
                .await
                .unwrap()
                .blocks_dal()
                .set_skip_proof_for_l1_batch(job_id)
                .await
                .unwrap();
        }
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        job: BasicWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<Option<BasicCircuitArtifacts>>> {
        let object_store = Arc::clone(&self.object_store);
        let config = self.config.clone();
        tokio::spawn(Self::process_job_impl(
            config,
            object_store,
            self.connection_pool.clone(),
            self.prover_connection_pool.clone(),
            job,
            started_at,
        ))
    }

    async fn save_result(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        optional_artifacts: Option<BasicCircuitArtifacts>,
    ) -> anyhow::Result<()> {
        match optional_artifacts {
            None => (),
            Some(artifacts) => {
                let blob_urls = save_artifacts(job_id, artifacts, &self.object_store).await;
                update_database(&self.prover_connection_pool, started_at, job_id, blob_urls).await;
            }
        }
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    async fn get_job_attempts(&self, _job_id: &Self::JobId) -> anyhow::Result<u32> {
        // Witness generator will be removed soon in favor of FRI one, so returning blank value.
        Ok(1)
    }
}

pub async fn process_basic_circuits_job(
    object_store: Arc<dyn ObjectStore>,
    config: WitnessGeneratorConfig,
    connection_pool: ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareBasicCircuitsJob,
) -> BasicCircuitArtifacts {
    let witness_gen_input =
        build_basic_circuits_witness_generator_input(connection_pool.clone(), job, block_number)
            .await;
    let (basic_circuits, basic_circuits_inputs, scheduler_witness) =
        generate_witness(object_store, config, connection_pool, witness_gen_input).await;
    let circuits = basic_circuits.clone().into_flattened_set();

    tracing::info!(
        "Witness generation for block {} is complete in {:?}. Number of circuits: {}",
        block_number.0,
        started_at.elapsed(),
        circuits.len()
    );

    BasicCircuitArtifacts {
        basic_circuits,
        basic_circuits_inputs,
        scheduler_witness,
        circuits,
    }
}

async fn update_database(
    prover_connection_pool: &ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    blob_urls: BlobUrls,
) {
    let mut prover_connection = prover_connection_pool.access_storage().await.unwrap();
    let mut transaction = prover_connection.start_transaction().await.unwrap();
    let protocol_version = transaction
        .witness_generator_dal()
        .protocol_version_for_l1_batch(block_number)
        .await
        .unwrap_or_else(|| {
            panic!(
                "No system version exist for l1 batch {} for basic circuits",
                block_number.0
            )
        });
    transaction
        .witness_generator_dal()
        .create_aggregation_jobs(
            block_number,
            &blob_urls.basic_circuits_url,
            &blob_urls.basic_circuits_inputs_url,
            blob_urls.circuit_types_and_urls.len(),
            &blob_urls.scheduler_witness_url,
            protocol_version,
        )
        .await;
    transaction
        .prover_dal()
        .insert_prover_jobs(
            block_number,
            blob_urls.circuit_types_and_urls,
            AggregationRound::BasicCircuits,
            protocol_version,
        )
        .await;
    transaction
        .witness_generator_dal()
        .mark_witness_job_as_successful(
            block_number,
            AggregationRound::BasicCircuits,
            started_at.elapsed(),
        )
        .await;

    transaction.commit().await.unwrap();
    METRICS.processing_time[&AggregationRound::BasicCircuits.into()].observe(started_at.elapsed());
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
) -> BlobUrls {
    let basic_circuits_url = object_store
        .put(block_number, &artifacts.basic_circuits)
        .await
        .unwrap();
    let basic_circuits_inputs_url = object_store
        .put(block_number, &artifacts.basic_circuits_inputs)
        .await
        .unwrap();
    let scheduler_witness_url = object_store
        .put(block_number, &artifacts.scheduler_witness)
        .await
        .unwrap();
    let circuit_types_and_urls = save_prover_input_artifacts(
        block_number,
        &artifacts.circuits,
        object_store,
        AggregationRound::BasicCircuits,
    )
    .await;
    BlobUrls {
        basic_circuits_url,
        basic_circuits_inputs_url,
        scheduler_witness_url,
        circuit_types_and_urls,
    }
}

// If making changes to this method, consider moving this logic to the DAL layer and make
// `PrepareBasicCircuitsJob` have all fields of `BasicCircuitWitnessGeneratorInput`.
pub async fn build_basic_circuits_witness_generator_input(
    connection_pool: ConnectionPool,
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
    let (previous_block_hash, previous_block_timestamp) = connection
        .blocks_dal()
        .get_l1_batch_state_root_and_timestamp(l1_batch_number - 1)
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

pub async fn generate_witness(
    object_store: Arc<dyn ObjectStore>,
    config: WitnessGeneratorConfig,
    connection_pool: ConnectionPool,
    input: BasicCircuitWitnessGeneratorInput,
) -> (
    BlockBasicCircuits<Bn256>,
    BlockBasicCircuitsPublicInputs<Bn256>,
    SchedulerCircuitInstanceWitness<Bn256>,
) {
    let mut connection = connection_pool.access_storage().await.unwrap();
    let header = connection
        .blocks_dal()
        .get_l1_batch_header(input.block_number)
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
        expand_memory_contents(&input.initial_heap_content, USED_BOOTLOADER_MEMORY_BYTES);
    let account_code_hash = h256_to_u256(header.base_system_contracts_hashes.default_aa);

    let hashes: HashSet<H256> = input
        .used_bytecodes_hashes
        .iter()
        // SMA-1555: remove this hack once updated to the latest version of zkevm_test_harness
        .filter(|&&hash| hash != h256_to_u256(header.base_system_contracts_hashes.bootloader))
        .map(|hash| u256_to_h256(*hash))
        .collect();

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
    let storage_refunds = connection
        .blocks_dal()
        .get_storage_refunds(input.block_number)
        .await
        .unwrap()
        .unwrap();

    drop(connection);
    let rt_handle = tokio::runtime::Handle::current();

    // The following part is CPU-heavy, so we move it to a separate thread.
    tokio::task::spawn_blocking(move || {
        // NOTE: this `match` will be moved higher up, as we need to load EVERYTHING from Blob, not just storage
        // Until we can derive Storage from Merkle Paths, we'll have this version as testing ground.
        let storage: Box<dyn ReadStorage> = match config.data_source {
            BasicWitnessGeneratorDataSource::FromPostgres => {
                let connection = rt_handle
                    .block_on(connection_pool.access_storage())
                    .unwrap();
                Box::new(PostgresStorage::new(
                    rt_handle.clone(),
                    connection,
                    last_miniblock_number,
                    true,
                ))
            }
            BasicWitnessGeneratorDataSource::FromPostgresShadowBlob => {
                let connection = rt_handle
                    .block_on(connection_pool.access_storage())
                    .unwrap();
                let block_state = rt_handle.block_on(object_store.get(header.number)).unwrap();
                let source_storage = Box::new(PostgresStorage::new(
                    rt_handle.clone(),
                    connection,
                    last_miniblock_number,
                    true,
                ));
                let checked_storage = Box::new(WitnessStorage::new(block_state));
                Box::new(ShadowStorage::new(
                    source_storage,
                    checked_storage,
                    input.block_number,
                ))
            }
            BasicWitnessGeneratorDataSource::FromBlob => {
                let block_state = rt_handle.block_on(object_store.get(header.number)).unwrap();
                Box::new(WitnessStorage::new(block_state))
            }
        };
        let mut tree = PrecalculatedMerklePathsProvider::new(
            input.merkle_paths_input,
            input.previous_block_hash.0,
        );

        let storage_view = StorageView::new(storage);
        let storage_view = storage_view.to_rc_ptr();
        let vm_storage_oracle: VmStorageOracle<StorageView<Box<dyn ReadStorage>>, HistoryDisabled> =
            VmStorageOracle::new(storage_view);
        let storage_oracle = StorageOracle::new(vm_storage_oracle, storage_refunds);
        let memory: SimpleMemory<HistoryDisabled> = SimpleMemory::default();
        let mut hasher = DefaultHasher::new();
        GEOMETRY_CONFIG.hash(&mut hasher);
        tracing::info!(
            "generating witness for block {} using geometry config hash: {}",
            input.block_number.0,
            hasher.finish()
        );

        if config
            .dump_arguments_for_blocks
            .contains(&input.block_number.0)
        {
            rt_handle.block_on(save_run_with_fixed_params_args_to_gcs(
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
                GEOMETRY_CONFIG,
                tree.clone(),
            ));
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
            memory,
            &mut tree,
        )
    })
    .await
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn save_run_with_fixed_params_args_to_gcs(
    object_store: Arc<dyn ObjectStore>,
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
