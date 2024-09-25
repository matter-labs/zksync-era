use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
    time::Instant,
};

use async_trait::async_trait;
use circuit_definitions::{
    circuit_definitions::base_layer::{ZkSyncBaseLayerCircuit, ZkSyncBaseLayerStorage},
    encodings::recursion_request::RecursionQueueSimulator,
    zkevm_circuits::{
        fsm_input_output::ClosedFormInputCompactFormWitness,
        scheduler::{
            block_header::BlockAuxilaryOutputWitness, input::SchedulerCircuitInstanceWitness,
        },
    },
};
use tokio::sync::Semaphore;
use tracing::Instrument;
use zkevm_test_harness::witness::oracle::WitnessGenerationArtifact;
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_multivm::{
    circuit_sequencer_api_latest::{
        boojum::{
            field::goldilocks::{GoldilocksExt2, GoldilocksField},
            gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
        },
        geometry_config::get_geometry_config,
    },
    interface::storage::StorageView,
    vm_latest::{constants::MAX_CYCLES_FOR_TX, HistoryDisabled, StorageOracle as VmStorageOracle},
    zk_evm_latest::ethereum_types::Address,
};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::{keys::ClosedFormInputKey, CircuitAuxData};
use zksync_prover_interface::inputs::WitnessInputData;
use zksync_prover_keystore::keystore::Keystore;
use zksync_system_constants::BOOTLOADER_ADDRESS;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchNumber,
};

use crate::{
    artifacts::ArtifactsManager,
    metrics::WITNESS_GENERATOR_METRICS,
    precalculated_merkle_paths_provider::PrecalculatedMerklePathsProvider,
    storage_oracle::StorageOracle,
    utils::{
        expand_bootloader_contents, save_circuit, save_ram_premutation_queue_witness,
        ClosedFormInputWrapper, KZG_TRUSTED_SETUP_FILE,
    },
    witness::WitnessStorage,
    witness_generator::WitnessGenerator,
};

mod artifacts;
pub mod job_processor;

#[derive(Clone)]
pub struct BasicCircuitArtifacts {
    pub(super) circuit_urls: Vec<(u8, String)>,
    pub(super) queue_urls: Vec<(u8, String, usize)>,
    pub(super) scheduler_witness: SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    pub(super) aux_output_witness: BlockAuxilaryOutputWitness<GoldilocksField>,
}

#[derive(Clone)]
pub struct BasicWitnessGeneratorJob {
    pub(super) block_number: L1BatchNumber,
    pub(super) data: WitnessInputData,
}

#[derive(Debug)]
pub struct BasicWitnessGenerator {
    config: Arc<FriWitnessGeneratorConfig>,
    object_store: Arc<dyn ObjectStore>,
    public_blob_store: Option<Arc<dyn ObjectStore>>,
    prover_connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
}

type Witness = (
    Vec<(u8, String)>,
    Vec<(u8, String, usize)>,
    SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    BlockAuxilaryOutputWitness<GoldilocksField>,
);

impl BasicWitnessGenerator {
    pub fn new(
        config: FriWitnessGeneratorConfig,
        object_store: Arc<dyn ObjectStore>,
        public_blob_store: Option<Arc<dyn ObjectStore>>,
        prover_connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
    ) -> Self {
        Self {
            config: Arc::new(config),
            object_store,
            public_blob_store,
            prover_connection_pool,
            protocol_version,
        }
    }
}

#[async_trait]
impl WitnessGenerator for BasicWitnessGenerator {
    type Job = BasicWitnessGeneratorJob;
    type Metadata = L1BatchNumber;
    type Artifacts = BasicCircuitArtifacts;

    async fn process_job(
        job: BasicWitnessGeneratorJob,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: Option<usize>,
        started_at: Instant,
    ) -> anyhow::Result<BasicCircuitArtifacts> {
        let BasicWitnessGeneratorJob {
            block_number,
            data: job,
        } = job;

        tracing::info!(
            "Starting witness generation of type {:?} for block {}",
            AggregationRound::BasicCircuits,
            block_number.0
        );

        let (circuit_urls, queue_urls, scheduler_witness, aux_output_witness) = generate_witness(
            block_number,
            object_store,
            job,
            max_circuits_in_flight.unwrap(),
        )
        .await;
        WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::BasicCircuits.into()]
            .observe(started_at.elapsed());
        tracing::info!(
            "Witness generation for block {} is complete in {:?}",
            block_number.0,
            started_at.elapsed()
        );

        Ok(BasicCircuitArtifacts {
            circuit_urls,
            queue_urls,
            scheduler_witness,
            aux_output_witness,
        })
    }

    async fn prepare_job(
        metadata: L1BatchNumber,
        object_store: &dyn ObjectStore,
        _keystore: Keystore,
    ) -> anyhow::Result<Self::Job> {
        tracing::info!("Processing FRI basic witness-gen for block {}", metadata.0);
        let started_at = Instant::now();
        let job = Self::get_artifacts(&metadata, object_store).await?;

        WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::BasicCircuits.into()]
            .observe(started_at.elapsed());

        Ok(job)
    }
}

#[tracing::instrument(skip_all, fields(l1_batch = %block_number))]
async fn generate_witness(
    block_number: L1BatchNumber,
    object_store: Arc<dyn ObjectStore>,
    input: WitnessInputData,
    max_circuits_in_flight: usize,
) -> Witness {
    let bootloader_contents = expand_bootloader_contents(
        &input.vm_run_data.initial_heap_content,
        input.vm_run_data.protocol_version,
    );

    let tree = PrecalculatedMerklePathsProvider::new(
        input.merkle_paths,
        input.previous_batch_metadata.root_hash.0,
    );
    let geometry_config = get_geometry_config();
    let mut hasher = DefaultHasher::new();
    geometry_config.hash(&mut hasher);
    tracing::info!(
        "generating witness for block {} using geometry config hash: {}",
        input.vm_run_data.l1_batch_number.0,
        hasher.finish()
    );

    let (circuit_sender, mut circuit_receiver) = tokio::sync::mpsc::channel(1);
    let (queue_sender, mut queue_receiver) = tokio::sync::mpsc::channel(1);
    let (ram_permutation_queue_sender, mut ram_permutation_queue_receiver) =
        tokio::sync::mpsc::channel(1);

    let make_circuits_span = tracing::info_span!("make_circuits");
    let make_circuits_span_copy = make_circuits_span.clone();
    // Blocking call from harness that does the CPU heavy lifting.
    // Provides circuits and recursion queue via callback functions and returns scheduler witnesses.
    // Circuits are "streamed" one by one as they're being generated.
    let make_circuits_handle = tokio::task::spawn_blocking(move || {
        let span = tracing::info_span!(parent: make_circuits_span_copy, "make_circuits_blocking");

        let witness_storage = WitnessStorage::new(input.vm_run_data.witness_block_state);
        let storage_view = StorageView::new(witness_storage).to_rc_ptr();

        let vm_storage_oracle: VmStorageOracle<StorageView<WitnessStorage>, HistoryDisabled> =
            VmStorageOracle::new(storage_view.clone());
        let storage_oracle = StorageOracle::new(
            vm_storage_oracle,
            input.vm_run_data.storage_refunds,
            input.vm_run_data.pubdata_costs,
        );

        let path = KZG_TRUSTED_SETUP_FILE
            .path()
            .to_str()
            .expect("Path to KZG trusted setup is not a UTF-8 string");

        let artifacts_callback = |artifact: WitnessGenerationArtifact| match artifact {
            WitnessGenerationArtifact::BaseLayerCircuit(circuit) => {
                let parent_span = span.clone();
                tracing::info_span!(parent: parent_span, "send_circuit").in_scope(|| {
                    circuit_sender
                        .blocking_send(circuit)
                        .expect("failed to send circuit from harness");
                });
            }
            WitnessGenerationArtifact::RecursionQueue((a, b, c)) => queue_sender
                .blocking_send((a as u8, b, c))
                .expect("failed to send recursion queue from harness"),
            a @ WitnessGenerationArtifact::MemoryQueueWitness(_) => {
                let parent_span = span.clone();
                tracing::info_span!(parent: parent_span, "send_ram_permutation_queue_witness")
                    .in_scope(|| {
                        ram_permutation_queue_sender
                            .blocking_send(a)
                            .expect("failed to send ram permutation queue sitness from harness");
                    });
            }
        };

        let (scheduler_witness, block_witness) = zkevm_test_harness::external_calls::run(
            Address::zero(),
            BOOTLOADER_ADDRESS,
            input.vm_run_data.bootloader_code,
            bootloader_contents,
            false,
            input.vm_run_data.default_account_code_hash,
            // NOTE: this will be evm_simulator_code_hash in future releases
            input.vm_run_data.default_account_code_hash,
            input.vm_run_data.used_bytecodes,
            Vec::default(),
            MAX_CYCLES_FOR_TX as usize,
            geometry_config,
            storage_oracle,
            tree,
            path,
            input.eip_4844_blobs.blobs(),
            artifacts_callback,
        );
        (scheduler_witness, block_witness)
    })
    .instrument(make_circuits_span);

    let semaphore = Arc::new(Semaphore::new(max_circuits_in_flight));

    let mut save_circuit_handles = vec![];

    let save_circuits_span = tracing::info_span!("save_circuits");

    // Future which receives circuits and saves them async.
    let circuit_receiver_handle = async {
        // Ordering determines how we compose the circuit proofs in Leaf Aggregation Round.
        // Sequence is used to determine circuit ordering (the sequencing of instructions) .
        // If the order is tampered with, proving will fail (as the proof would be computed for a different sequence of instruction).
        let mut circuit_sequence = 0;

        let mut ram_circuit_sequence = 0;

        while let Some(circuit) = circuit_receiver
            .recv()
            .instrument(tracing::info_span!("wait_for_circuit"))
            .await
        {
            let sequence = circuit_sequence;
            circuit_sequence += 1;
            let object_store = object_store.clone();
            let semaphore = semaphore.clone();
            let permit = semaphore
                .acquire_owned()
                .await
                .expect("failed to get permit for running save circuit task");

            let partial_circuit_aux_data = match &circuit {
                ZkSyncBaseLayerCircuit::RAMPermutation(_) => {
                    let circuit_subsequence_number = ram_circuit_sequence;
                    ram_circuit_sequence += 1;
                    Some(CircuitAuxData {
                        circuit_subsequence_number,
                    })
                }
                _ => None,
            };

            save_circuit_handles.push(tokio::task::spawn(async move {
                let (circuit_id, circuit_url) = save_circuit(
                    block_number,
                    circuit,
                    sequence,
                    partial_circuit_aux_data,
                    object_store,
                )
                .await;
                drop(permit);
                (circuit_id, circuit_url)
            }));
        }
    }
    .instrument(save_circuits_span);

    let mut save_ram_queue_witness_handles = vec![];

    let save_ram_queue_witness_span = tracing::info_span!("save_circuits");

    // Future which receives part of RAM permutation circuits witnesses and saves them async.
    // Uses semaphore because these artifacts are of significant size
    let ram_queue_witness_receiver_handle = async {
        let mut sorted_sequence = 0;
        let mut unsorted_sequence = 0;

        while let Some(witness_artifact) = ram_permutation_queue_receiver
            .recv()
            .instrument(tracing::info_span!("wait_for_ram_witness"))
            .await
        {
            let object_store = object_store.clone();
            let semaphore = semaphore.clone();
            let permit = semaphore
                .acquire_owned()
                .await
                .expect("failed to get permit for running save ram permutation queue witness task");
            let (is_sorted, witness, sequence) = match witness_artifact {
                WitnessGenerationArtifact::MemoryQueueWitness((witness, sorted)) => {
                    let sequence = if sorted {
                        let sequence = sorted_sequence;
                        sorted_sequence += 1;
                        sequence
                    } else {
                        let sequence = unsorted_sequence;
                        unsorted_sequence += 1;
                        sequence
                    };
                    (sorted, witness, sequence)
                }
                _ => panic!("Invalid artifact received"),
            };
            save_ram_queue_witness_handles.push(tokio::task::spawn(async move {
                let _ = save_ram_premutation_queue_witness(
                    block_number,
                    sequence,
                    is_sorted,
                    witness,
                    object_store,
                )
                .await;
                drop(permit);
            }));
        }
    }
    .instrument(save_ram_queue_witness_span);

    let mut save_queue_handles = vec![];

    let save_queues_span = tracing::info_span!("save_queues");

    // Future which receives recursion queues and saves them async.
    // Note that this section needs no semaphore as there's # of circuit ids (16) queues at most.
    // All queues combined are < 10MB.
    let queue_receiver_handle = async {
        while let Some((circuit_id, queue, inputs)) = queue_receiver
            .recv()
            .instrument(tracing::info_span!("wait_for_queue"))
            .await
        {
            let object_store = object_store.clone();
            save_queue_handles.push(tokio::task::spawn(save_recursion_queue(
                block_number,
                circuit_id,
                queue,
                inputs,
                object_store,
            )));
        }
    }
    .instrument(save_queues_span);

    let (witnesses, _, _, _) = tokio::join!(
        make_circuits_handle,
        circuit_receiver_handle,
        queue_receiver_handle,
        ram_queue_witness_receiver_handle
    );
    let (mut scheduler_witness, block_aux_witness) = witnesses.unwrap();

    // Harness returns recursion queues for all circuits, but for proving only the queues that have circuits matter.
    // `circuits_present` stores which circuits exist and is used to filter queues in `recursion_urls` later.
    let mut circuits_present = HashSet::<u8>::new();

    let circuit_urls = futures::future::join_all(save_circuit_handles)
        .await
        .into_iter()
        .map(|result| {
            let (circuit_id, circuit_url) = result.expect("failed to save circuit");
            circuits_present.insert(circuit_id);
            (circuit_id, circuit_url)
        })
        .collect();

    let recursion_urls = futures::future::join_all(save_queue_handles)
        .await
        .into_iter()
        .map(|result| result.expect("failed to save queue"))
        .filter(|(circuit_id, _, _)| circuits_present.contains(circuit_id))
        .collect();

    let _: Vec<_> = futures::future::join_all(save_ram_queue_witness_handles)
        .await
        .into_iter()
        .map(|result| result.expect("failed to save ram permutation queue witness"))
        .collect();

    scheduler_witness.previous_block_meta_hash = input.previous_batch_metadata.meta_hash.0;
    scheduler_witness.previous_block_aux_hash = input.previous_batch_metadata.aux_hash.0;

    (
        circuit_urls,
        recursion_urls,
        scheduler_witness,
        block_aux_witness,
    )
}

#[tracing::instrument(skip_all, fields(l1_batch = %block_number, circuit_id = %circuit_id))]
async fn save_recursion_queue(
    block_number: L1BatchNumber,
    circuit_id: u8,
    recursion_queue_simulator: RecursionQueueSimulator<GoldilocksField>,
    closed_form_inputs: Vec<ClosedFormInputCompactFormWitness<GoldilocksField>>,
    object_store: Arc<dyn ObjectStore>,
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
