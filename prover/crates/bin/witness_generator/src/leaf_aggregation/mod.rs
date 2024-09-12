use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use circuit_definitions::circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type;
use tokio::sync::Semaphore;
use zkevm_test_harness::{
    witness::recursive_aggregation::{
        compute_leaf_params, create_leaf_witness, split_recursion_queue,
    },
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::base_layer::{
            ZkSyncBaseLayerClosedFormInput, ZkSyncBaseLayerVerificationKey,
        },
        encodings::recursion_request::RecursionQueueSimulator,
        zkevm_circuits::recursion::leaf_layer::input::RecursionLeafParametersWitness,
    },
    FriProofWrapper,
};
use zksync_prover_keystore::keystore::Keystore;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion,
    prover_dal::LeafAggregationJobMetadata, L1BatchNumber,
};

use crate::{
    artifacts::ArtifactsManager,
    metrics::WITNESS_GENERATOR_METRICS,
    utils::{
        load_proofs_for_job_ids, save_recursive_layer_prover_input_artifacts,
        ClosedFormInputWrapper,
    },
};

mod artifacts;
mod job_processor;

pub struct LeafAggregationWitnessGeneratorJob {
    pub(crate) circuit_id: u8,
    pub(crate) block_number: L1BatchNumber,
    pub(crate) closed_form_inputs: ClosedFormInputWrapper,
    pub(crate) proofs_ids: Vec<u32>,
    pub(crate) base_vk: ZkSyncBaseLayerVerificationKey,
    pub(crate) leaf_params: RecursionLeafParametersWitness<GoldilocksField>,
}

#[derive(Debug)]
pub struct LeafAggregationWitnessGenerator {
    config: FriWitnessGeneratorConfig,
    object_store: Arc<dyn ObjectStore>,
    prover_connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
    keystore: Keystore,
}

#[derive(Clone)]
pub struct LeafAggregationArtifacts {
    circuit_id: u8,
    block_number: L1BatchNumber,
    pub aggregations: Vec<(u64, RecursionQueueSimulator<GoldilocksField>)>,
    pub circuit_ids_and_urls: Vec<(u8, String)>,
    #[allow(dead_code)]
    closed_form_inputs: Vec<ZkSyncBaseLayerClosedFormInput<GoldilocksField>>,
}

impl LeafAggregationWitnessGenerator {
    pub fn new(
        config: FriWitnessGeneratorConfig,
        object_store: Arc<dyn ObjectStore>,
        prover_connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
        keystore: Keystore,
    ) -> Self {
        Self {
            config,
            object_store,
            prover_connection_pool,
            protocol_version,
            keystore,
        }
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %leaf_job.block_number, circuit_id = %leaf_job.circuit_id)
    )]
    pub async fn process_job_impl(
        leaf_job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
    ) -> LeafAggregationArtifacts {
        tracing::info!(
            "Starting witness generation of type {:?} for block {} with circuit {}",
            AggregationRound::LeafAggregation,
            leaf_job.block_number.0,
            leaf_job.circuit_id,
        );
        process_leaf_aggregation_job(started_at, leaf_job, object_store, max_circuits_in_flight)
            .await
    }
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %metadata.block_number, circuit_id = %metadata.circuit_id)
)]
pub async fn prepare_leaf_aggregation_job(
    metadata: LeafAggregationJobMetadata,
    object_store: &dyn ObjectStore,
    keystore: Keystore,
) -> anyhow::Result<LeafAggregationWitnessGeneratorJob> {
    let started_at = Instant::now();
    let closed_form_input =
        LeafAggregationWitnessGenerator::get_artifacts(&metadata, object_store).await?;

    WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::LeafAggregation.into()]
        .observe(started_at.elapsed());

    let started_at = Instant::now();
    let base_vk = keystore
        .load_base_layer_verification_key(metadata.circuit_id)
        .context("get_base_layer_vk_for_circuit_type()")?;

    let leaf_circuit_id = base_circuit_type_into_recursive_leaf_circuit_type(
        BaseLayerCircuitType::from_numeric_value(metadata.circuit_id),
    ) as u8;

    let leaf_vk = keystore
        .load_recursive_layer_verification_key(leaf_circuit_id)
        .context("get_recursive_layer_vk_for_circuit_type()")?;
    let leaf_params = compute_leaf_params(metadata.circuit_id, base_vk.clone(), leaf_vk);

    WITNESS_GENERATOR_METRICS.prepare_job_time[&AggregationRound::LeafAggregation.into()]
        .observe(started_at.elapsed());

    Ok(LeafAggregationWitnessGeneratorJob {
        circuit_id: metadata.circuit_id,
        block_number: metadata.block_number,
        closed_form_inputs: closed_form_input,
        proofs_ids: metadata.prover_job_ids_for_proofs,
        base_vk,
        leaf_params,
    })
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %job.block_number, circuit_id = %job.circuit_id)
)]
pub async fn process_leaf_aggregation_job(
    started_at: Instant,
    job: LeafAggregationWitnessGeneratorJob,
    object_store: Arc<dyn ObjectStore>,
    max_circuits_in_flight: usize,
) -> LeafAggregationArtifacts {
    let circuit_id = job.circuit_id;
    let queues = split_recursion_queue(job.closed_form_inputs.1);

    assert_eq!(circuit_id, job.base_vk.numeric_circuit_type());

    let aggregations = queues
        .iter()
        .cloned()
        .map(|queue| (circuit_id as u64, queue))
        .collect();

    let mut proof_ids_iter = job.proofs_ids.into_iter();
    let mut proofs_ids = vec![];
    for queue in queues.iter() {
        let proofs_ids_for_queue: Vec<_> = (&mut proof_ids_iter)
            .take(queue.num_items as usize)
            .collect();
        assert_eq!(queue.num_items as usize, proofs_ids_for_queue.len());
        proofs_ids.push(proofs_ids_for_queue);
    }

    let semaphore = Arc::new(Semaphore::new(max_circuits_in_flight));

    let mut handles = vec![];
    for (circuit_idx, (queue, proofs_ids_for_queue)) in
        queues.into_iter().zip(proofs_ids).enumerate()
    {
        let semaphore = semaphore.clone();

        let object_store = object_store.clone();
        let queue = queue.clone();
        let base_vk = job.base_vk.clone();
        let leaf_params = (circuit_id, job.leaf_params.clone());

        let handle = tokio::task::spawn(async move {
            let _permit = semaphore
                .acquire()
                .await
                .expect("failed to get permit to process queues chunk");

            let proofs = load_proofs_for_job_ids(&proofs_ids_for_queue, &*object_store).await;
            let base_proofs = proofs
                .into_iter()
                .map(|wrapper| match wrapper {
                    FriProofWrapper::Base(base_proof) => base_proof,
                    FriProofWrapper::Recursive(_) => {
                        panic!(
                            "Expected only base proofs for leaf agg {} {}",
                            job.circuit_id, job.block_number
                        );
                    }
                })
                .collect();

            let (_, circuit) = create_leaf_witness(
                circuit_id.into(),
                queue,
                base_proofs,
                &base_vk,
                &leaf_params,
            );

            save_recursive_layer_prover_input_artifacts(
                job.block_number,
                circuit_idx,
                vec![circuit],
                AggregationRound::LeafAggregation,
                0,
                &*object_store,
                None,
            )
            .await
        });

        handles.push(handle);
    }

    let circuit_ids_and_urls_results = futures::future::join_all(handles).await;
    let circuit_ids_and_urls = circuit_ids_and_urls_results
        .into_iter()
        .flat_map(|x| x.unwrap())
        .collect();

    WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::LeafAggregation.into()]
        .observe(started_at.elapsed());

    tracing::info!(
        "Leaf witness generation for block {} with circuit id {}: is complete in {:?}.",
        job.block_number.0,
        circuit_id,
        started_at.elapsed(),
    );

    LeafAggregationArtifacts {
        circuit_id,
        block_number: job.block_number,
        aggregations,
        circuit_ids_and_urls,
        closed_form_inputs: job.closed_form_inputs.0,
    }
}
