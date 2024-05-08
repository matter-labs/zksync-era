use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    hash::{Hash, Hasher},
    sync::Arc,
    time::Instant,
};

use anyhow::Context as _;
use async_trait::async_trait;
use circuit_definitions::{
    circuit_definitions::base_layer::ZkSyncBaseLayerStorage,
    encodings::recursion_request::RecursionQueueSimulator,
    zkevm_circuits::fsm_input_output::ClosedFormInputCompactFormWitness,
};
use multivm::vm_latest::{
    constants::MAX_CYCLES_FOR_TX, HistoryDisabled, StorageOracle as VmStorageOracle,
};
use prover_dal::{ConnectionPool, Prover, ProverDal};
use tracing::Instrument;
use zkevm_test_harness::geometry_config::get_geometry_config;
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_dal::{Core, CoreDal};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::{
            field::goldilocks::{GoldilocksExt2, GoldilocksField},
            gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
        },
        zkevm_circuits::scheduler::{
            block_header::BlockAuxilaryOutputWitness, input::SchedulerCircuitInstanceWitness,
        },
    },
    get_current_pod_name,
    keys::ClosedFormInputKey,
    AuxOutputWitnessWrapper,
};
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_prover_interface::inputs::{BasicCircuitWitnessGeneratorInput, PrepareBasicCircuitsJob};
use zksync_queued_job_processor::JobProcessor;
use zksync_state::{PostgresStorage, StorageView};
use zksync_types::{
    basic_fri_types::{AggregationRound, Eip4844Blobs},
    block::StorageOracleInfo,
    protocol_version::ProtocolVersionId,
    Address, L1BatchNumber, BOOTLOADER_ADDRESS, H256,
};
use zksync_utils::{bytes_to_chunks, h256_to_u256, u256_to_h256};

use crate::{
    metrics::WITNESS_GENERATOR_METRICS,
    precalculated_merkle_paths_provider::PrecalculatedMerklePathsProvider,
    storage_oracle::StorageOracle,
    utils::{
        expand_bootloader_contents, save_circuit, ClosedFormInputWrapper,
        SchedulerPartialInputWrapper, KZG_TRUSTED_SETUP_FILE,
    },
};

pub struct BasicCircuitArtifacts {
    circuit_urls: Vec<(u8, String)>,
    queue_urls: Vec<(u8, String, usize)>,
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
    eip_4844_blobs: Eip4844Blobs,
}

#[derive(Debug)]
pub struct BasicWitnessGenerator {
    config: Arc<FriWitnessGeneratorConfig>,
    object_store: Arc<dyn ObjectStore>,
    public_blob_store: Option<Arc<dyn ObjectStore>>,
    connection_pool: ConnectionPool<Core>,
    prover_connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolVersionId,
}

impl BasicWitnessGenerator {
    pub async fn new(
        config: FriWitnessGeneratorConfig,
        store_factory: &ObjectStoreFactory,
        public_blob_store: Option<Arc<dyn ObjectStore>>,
        connection_pool: ConnectionPool<Core>,
        prover_connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolVersionId,
    ) -> Self {
        Self {
            config: Arc::new(config),
            object_store: store_factory.create_store().await,
            public_blob_store,
            connection_pool,
            prover_connection_pool,
            protocol_version,
        }
    }

    async fn process_job_impl(
        object_store: Arc<dyn ObjectStore>,
        connection_pool: ConnectionPool<Core>,
        basic_job: BasicWitnessGeneratorJob,
        started_at: Instant,
    ) -> Option<BasicCircuitArtifacts> {
        let BasicWitnessGeneratorJob {
            block_number,
            job,
            eip_4844_blobs,
        } = basic_job;

        tracing::info!(
            "Starting witness generation of type {:?} for block {}",
            AggregationRound::BasicCircuits,
            block_number.0
        );

        Some(
            process_basic_circuits_job(
                &*object_store,
                connection_pool,
                started_at,
                block_number,
                job,
                eip_4844_blobs,
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
        let mut prover_connection = self.prover_connection_pool.connection().await?;
        let last_l1_batch_to_process = self.config.last_l1_batch_to_process();
        let pod_name = get_current_pod_name();
        match prover_connection
            .fri_witness_generator_dal()
            .get_next_basic_circuit_witness_job(
                last_l1_batch_to_process,
                self.protocol_version,
                &pod_name,
            )
            .await
        {
            Some((block_number, eip_4844_blobs)) => {
                tracing::info!(
                    "Processing FRI basic witness-gen for block {}",
                    block_number
                );
                let started_at = Instant::now();
                let job = get_artifacts(block_number, &*self.object_store, eip_4844_blobs).await;

                WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::BasicCircuits.into()]
                    .observe(started_at.elapsed());

                Ok(Some((block_number, job)))
            }
            None => Ok(None),
        }
    }

    async fn save_failure(&self, job_id: L1BatchNumber, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_witness_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: BasicWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<Option<BasicCircuitArtifacts>>> {
        let object_store = Arc::clone(&self.object_store);
        let connection_pool = self.connection_pool.clone();
        tokio::spawn(async move {
            let block_number = job.block_number;
            Ok(
                Self::process_job_impl(object_store, connection_pool, job, started_at)
                    .instrument(tracing::info_span!("basic_circuit", %block_number))
                    .await,
            )
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
                let scheduler_witness_url = save_scheduler_artifacts(
                    job_id,
                    artifacts.scheduler_witness,
                    artifacts.aux_output_witness,
                    &*self.object_store,
                    self.public_blob_store.as_deref(),
                    self.config.shall_save_to_public_bucket,
                )
                .await;

                WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::BasicCircuits.into()]
                    .observe(blob_started_at.elapsed());

                update_database(
                    &self.prover_connection_pool,
                    started_at,
                    job_id,
                    BlobUrls {
                        circuit_ids_and_urls: artifacts.circuit_urls,
                        closed_form_inputs_and_urls: artifacts.queue_urls,
                        scheduler_witness_url,
                    },
                )
                .await;
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
            .connection()
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

#[allow(clippy::too_many_arguments)]
async fn process_basic_circuits_job(
    object_store: &dyn ObjectStore,
    connection_pool: ConnectionPool<Core>,
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareBasicCircuitsJob,
    eip_4844_blobs: Eip4844Blobs,
) -> BasicCircuitArtifacts {
    let witness_gen_input =
        build_basic_circuits_witness_generator_input(&connection_pool, job, block_number).await;
    let (circuit_urls, queue_urls, scheduler_witness, aux_output_witness) = generate_witness(
        block_number,
        object_store,
        connection_pool,
        witness_gen_input,
        eip_4844_blobs,
    )
    .await;
    WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::BasicCircuits.into()]
        .observe(started_at.elapsed());
    tracing::info!(
        "Witness generation for block {} is complete in {:?}",
        block_number.0,
        started_at.elapsed()
    );

    BasicCircuitArtifacts {
        circuit_urls,
        queue_urls,
        scheduler_witness,
        aux_output_witness,
    }
}

async fn update_database(
    prover_connection_pool: &ConnectionPool<Prover>,
    started_at: Instant,
    block_number: L1BatchNumber,
    blob_urls: BlobUrls,
) {
    let mut prover_connection = prover_connection_pool.connection().await.unwrap();
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
    eip_4844_blobs: Eip4844Blobs,
) -> BasicWitnessGeneratorJob {
    let job = object_store.get(block_number).await.unwrap();
    BasicWitnessGeneratorJob {
        block_number,
        job,
        eip_4844_blobs,
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

async fn save_recursion_queue(
    block_number: L1BatchNumber,
    circuit_id: u8,
    recursion_queue_simulator: RecursionQueueSimulator<GoldilocksField>,
    closed_form_inputs: &[ClosedFormInputCompactFormWitness<GoldilocksField>],
    object_store: &dyn ObjectStore,
) -> (u8, String, usize) {
    let key = ClosedFormInputKey {
        block_number,
        circuit_id,
    };
    let basic_circuit_count = closed_form_inputs.len();
    let closed_form_inputs = closed_form_inputs
        .iter()
        .map(|x| ZkSyncBaseLayerStorage::from_inner(circuit_id, x.clone()))
        .collect();
    let wrapper = ClosedFormInputWrapper(closed_form_inputs, recursion_queue_simulator);
    let blob_url = object_store.put(key, &wrapper).await.unwrap();
    (circuit_id, blob_url, basic_circuit_count)
}

// If making changes to this method, consider moving this logic to the DAL layer and make
// `PrepareBasicCircuitsJob` have all fields of `BasicCircuitWitnessGeneratorInput`.
async fn build_basic_circuits_witness_generator_input(
    connection_pool: &ConnectionPool<Core>,
    witness_merkle_input: PrepareBasicCircuitsJob,
    l1_batch_number: L1BatchNumber,
) -> BasicCircuitWitnessGeneratorInput {
    let mut connection = connection_pool.connection().await.unwrap();
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
    block_number: L1BatchNumber,
    object_store: &dyn ObjectStore,
    connection_pool: ConnectionPool<Core>,
    input: BasicCircuitWitnessGeneratorInput,
    eip_4844_blobs: Eip4844Blobs,
) -> (
    Vec<(u8, String)>,
    Vec<(u8, String, usize)>,
    SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    BlockAuxilaryOutputWitness<GoldilocksField>,
) {
    let mut connection = connection_pool.connection().await.unwrap();
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
        .factory_deps_dal()
        .get_factory_dep(header.base_system_contracts_hashes.bootloader)
        .await
        .expect("Failed fetching bootloader bytecode from DB")
        .expect("Bootloader bytecode should exist");
    let bootloader_code = bytes_to_chunks(&bootloader_code_bytes);
    let account_bytecode_bytes = connection
        .factory_deps_dal()
        .get_factory_dep(header.base_system_contracts_hashes.default_aa)
        .await
        .expect("Failed fetching default account bytecode from DB")
        .expect("Default account bytecode should exist");
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

    let StorageOracleInfo {
        storage_refunds,
        pubdata_costs,
    } = connection
        .blocks_dal()
        .get_storage_oracle_info(input.block_number)
        .await
        .unwrap()
        .unwrap();

    let mut used_bytecodes = connection
        .factory_deps_dal()
        .get_factory_deps(&hashes)
        .await;
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
        .get_l2_block_range_of_l1_batch(input.block_number - 1)
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

    // The following part is CPU-heavy, so we move it to a separate thread.
    let rt_handle = tokio::runtime::Handle::current();

    let (circuit_sender, mut circuit_receiver) = tokio::sync::mpsc::channel(1);
    let (queue_sender, mut queue_receiver) = tokio::sync::mpsc::channel(1);

    let make_circuits = tokio::task::spawn_blocking(move || {
        let connection = rt_handle.block_on(connection_pool.connection()).unwrap();

        let storage = PostgresStorage::new(rt_handle, connection, last_miniblock_number, true);
        let storage_view = StorageView::new(storage).to_rc_ptr();

        let vm_storage_oracle: VmStorageOracle<StorageView<PostgresStorage<'_>>, HistoryDisabled> =
            VmStorageOracle::new(storage_view.clone());
        let storage_oracle = StorageOracle::new(
            vm_storage_oracle,
            storage_refunds,
            pubdata_costs.expect("pubdata costs should be present"),
        );

        let path = KZG_TRUSTED_SETUP_FILE
            .path()
            .to_str()
            .expect("Path to KZG trusted setup is not a UTF-8 string");

        let (scheduler_witness, block_witness) = zkevm_test_harness::external_calls::run(
            Address::zero(),
            BOOTLOADER_ADDRESS,
            bootloader_code,
            bootloader_contents,
            false,
            account_code_hash,
            // NOTE: this will be evm_simulator_code_hash in future releases
            account_code_hash,
            used_bytecodes,
            Vec::default(),
            MAX_CYCLES_FOR_TX as usize,
            geometry_config,
            storage_oracle,
            &mut tree,
            path,
            eip_4844_blobs.blobs(),
            |circuit| {
                circuit_sender.blocking_send(circuit).unwrap();
            },
            |a, b, c| queue_sender.blocking_send((a as u8, b, c)).unwrap(),
        );
        (scheduler_witness, block_witness)
    });

    let mut circuit_urls = vec![];
    let mut recursion_urls = vec![];

    let mut circuits_present = HashSet::<u8>::new();

    let save_circuits = async {
        loop {
            tokio::select! {
                Some(circuit) = circuit_receiver.recv() => {
                    circuits_present.insert(circuit.numeric_circuit_type());
                    circuit_urls.push(
                        save_circuit(block_number, circuit, circuit_urls.len(), object_store).await,
                    );
                }
                Some((circuit_id, queue, inputs)) = queue_receiver.recv() => recursion_urls.push(
                    save_recursion_queue(block_number, circuit_id, queue, &inputs, object_store)
                        .await,
                ),
                else => break,
            };
        }
    };

    let (witnesses, ()) = tokio::join!(make_circuits, save_circuits);
    let (mut scheduler_witness, block_aux_witness) = witnesses.unwrap();

    recursion_urls.retain(|(circuit_id, _, _)| circuits_present.contains(circuit_id));

    scheduler_witness.previous_block_meta_hash =
        previous_batch_with_metadata.metadata.meta_parameters_hash.0;
    scheduler_witness.previous_block_aux_hash =
        previous_batch_with_metadata.metadata.aux_data_hash.0;

    (
        circuit_urls,
        recursion_urls,
        scheduler_witness,
        block_aux_witness,
    )
}
