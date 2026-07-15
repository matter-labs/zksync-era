use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use circuit_definitions::circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type;
use tokio::sync::Semaphore;
use zkevm_test_harness::{
    witness::recursive_aggregation::{
        compute_leaf_params, create_leaf_witness, split_recursion_queue,
    },
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::base_layer::{
            ZkSyncBaseLayerClosedFormInput, ZkSyncBaseLayerVerificationKey,
        },
        encodings::recursion_request::RecursionQueueSimulator,
        zkevm_circuits::recursion::leaf_layer::input::RecursionLeafParametersWitness,
    },
    get_current_pod_name, FriProofWrapper,
};
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion,
    prover_dal::LeafAggregationJobMetadata, L1BatchId,
};

use super::JobMetadata;
use crate::{
    artifact_manager::{ArtifactsManager, JobId},
    metrics::WITNESS_GENERATOR_METRICS,
    rounds::{JobManager, VerificationKeyManager},
    utils::{
        load_proofs_for_job_ids, save_recursive_layer_prover_input_artifacts,
        ClosedFormInputWrapper,
    },
};

mod artifacts;

pub struct LeafAggregationWitnessGeneratorJob {
    pub(crate) circuit_id: u8,
    pub(crate) batch_id: L1BatchId,
    pub(crate) closed_form_inputs: ClosedFormInputWrapper,
    pub(crate) proofs_ids: Vec<u32>,
    pub(crate) base_vk: ZkSyncBaseLayerVerificationKey,
    pub(crate) leaf_params: RecursionLeafParametersWitness<GoldilocksField>,
}

#[derive(Clone)]
pub struct LeafAggregationArtifacts {
    circuit_id: u8,
    batch_id: L1BatchId,
    pub aggregations: Vec<(u64, RecursionQueueSimulator<GoldilocksField>)>,
    pub circuit_ids_sequence_numbers_and_urls: Vec<(u8, usize, String)>,
    #[allow(dead_code)]
    closed_form_inputs: Vec<ZkSyncBaseLayerClosedFormInput<GoldilocksField>>,
}

pub struct LeafAggregation;

#[async_trait]
impl JobManager for LeafAggregation {
    type Job = LeafAggregationWitnessGeneratorJob;
    type Metadata = (LeafAggregationJobMetadata, Instant);

    const ROUND: AggregationRound = AggregationRound::LeafAggregation;
    const SERVICE_NAME: &'static str = "fri_leaf_aggregation_witness_generator";

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %job.batch_id, circuit_id = %job.circuit_id)
    )]
    async fn process_job(
        job: LeafAggregationWitnessGeneratorJob,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
    ) -> anyhow::Result<LeafAggregationArtifacts> {
        let started_at = Instant::now();
        tracing::info!(
            "Starting witness generation of type {:?} for block {} with circuit {}",
            AggregationRound::LeafAggregation,
            job.batch_id,
            job.circuit_id,
        );
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

                let job_ids: Vec<JobId> = proofs_ids_for_queue
                    .iter()
                    .map(|id| JobId::new(*id, job.batch_id.chain_id()))
                    .collect();
                let proofs = load_proofs_for_job_ids(&job_ids, &*object_store).await;
                let base_proofs = proofs
                    .into_iter()
                    .map(|wrapper| match wrapper {
                        FriProofWrapper::Base(base_proof) => base_proof,
                        FriProofWrapper::Recursive(_) => {
                            panic!(
                                "Expected only base proofs for leaf agg {} {}",
                                job.circuit_id, job.batch_id
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
                    job.batch_id,
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

        let circuit_ids_sequence_numbers_and_urls_results =
            futures::future::join_all(handles).await;
        let circuit_ids_sequence_numbers_and_urls = circuit_ids_sequence_numbers_and_urls_results
            .into_iter()
            .flat_map(|x| x.unwrap())
            .collect();

        tracing::info!(
            "Leaf witness generation for block {} with circuit id {}: is complete in {:?}.",
            job.batch_id,
            circuit_id,
            started_at.elapsed(),
        );

        Ok(LeafAggregationArtifacts {
            circuit_id,
            batch_id: job.batch_id,
            aggregations,
            circuit_ids_sequence_numbers_and_urls,
            closed_form_inputs: job.closed_form_inputs.0,
        })
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %metadata.0.batch_id, circuit_id = %metadata.0.circuit_id)
    )]
    async fn prepare_job(
        metadata: Self::Metadata,
        object_store: &dyn ObjectStore,
        keystore: Arc<dyn VerificationKeyManager>,
    ) -> anyhow::Result<LeafAggregationWitnessGeneratorJob> {
        let (metadata, started_at) = metadata;
        let closed_form_input = Self::get_artifacts(&metadata, object_store).await?;

        WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::LeafAggregation.into()]
            .observe(started_at.elapsed());

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

        Ok(LeafAggregationWitnessGeneratorJob {
            circuit_id: metadata.circuit_id,
            batch_id: metadata.batch_id,
            closed_form_inputs: closed_form_input,
            proofs_ids: metadata.prover_job_ids_for_proofs,
            base_vk,
            leaf_params,
        })
    }

    async fn get_metadata(
        connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
    ) -> anyhow::Result<Option<Self::Metadata>> {
        let pod_name = get_current_pod_name();
        let Some(metadata) = connection_pool
            .connection()
            .await?
            .fri_leaf_witness_generator_dal()
            .get_next_leaf_aggregation_job(protocol_version, &pod_name)
            .await
        else {
            return Ok(None);
        };
        let started_at = Instant::now();
        Ok(Some((metadata, started_at)))
    }
}

impl JobMetadata for (LeafAggregationJobMetadata, Instant) {
    fn job_id(&self) -> JobId {
        JobId::new(self.0.id, self.0.batch_id.chain_id())
    }
    fn started_at(&self) -> Instant {
        self.1
    }
}
