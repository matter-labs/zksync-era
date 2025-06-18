use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use circuit_definitions::circuit_definitions::recursion_layer::RECURSION_ARITY;
use tokio::sync::Semaphore;
use zkevm_test_harness::witness::recursive_aggregation::{
    compute_node_vk_commitment, create_node_witness,
};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::recursion_layer::{
            ZkSyncRecursionLayerStorageType, ZkSyncRecursionLayerVerificationKey,
        },
        encodings::recursion_request::RecursionQueueSimulator,
        zkevm_circuits::recursion::leaf_layer::input::RecursionLeafParametersWitness,
    },
    get_current_pod_name, FriProofWrapper,
};
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion,
    prover_dal::NodeAggregationJobMetadata, L1BatchId,
};

use super::JobMetadata;
use crate::{
    artifact_manager::{ArtifactsManager, JobId},
    metrics::WITNESS_GENERATOR_METRICS,
    rounds::{JobManager, VerificationKeyManager},
    utils::{load_proofs_for_job_ids, save_recursive_layer_prover_input_artifacts},
};
mod artifacts;

#[derive(Clone)]
pub struct NodeAggregationArtifacts {
    circuit_id: u8,
    batch_id: L1BatchId,
    depth: u16,
    pub next_aggregations: Vec<(u64, RecursionQueueSimulator<GoldilocksField>)>,
    pub recursive_circuit_ids_and_urls: Vec<(u8, String)>,
}

#[derive(Clone)]
pub struct NodeAggregationWitnessGeneratorJob {
    circuit_id: u8,
    batch_id: L1BatchId,
    depth: u16,
    aggregations: Vec<(u64, RecursionQueueSimulator<GoldilocksField>)>,
    proofs_ids: Vec<u32>,
    leaf_vk: ZkSyncRecursionLayerVerificationKey,
    node_vk: ZkSyncRecursionLayerVerificationKey,
    all_leafs_layer_params: Vec<(u8, RecursionLeafParametersWitness<GoldilocksField>)>,
}

pub struct NodeAggregation;

#[async_trait]
impl JobManager for NodeAggregation {
    type Job = NodeAggregationWitnessGeneratorJob;
    type Metadata = (NodeAggregationJobMetadata, Instant);

    const ROUND: AggregationRound = AggregationRound::NodeAggregation;
    const SERVICE_NAME: &'static str = "fri_node_aggregation_witness_generator";

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = % job.batch_id, circuit_id = % job.circuit_id)
    )]
    async fn process_job(
        job: NodeAggregationWitnessGeneratorJob,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
    ) -> anyhow::Result<NodeAggregationArtifacts> {
        let started_at = Instant::now();
        let node_vk_commitment = compute_node_vk_commitment(job.node_vk.clone());
        tracing::info!(
            "Starting witness generation of type {:?} for block {} circuit id {} depth {}",
            AggregationRound::NodeAggregation,
            job.batch_id,
            job.circuit_id,
            job.depth
        );
        let vk = match job.depth {
            0 => job.leaf_vk,
            _ => job.node_vk,
        };

        let mut proof_ids_iter = job.proofs_ids.into_iter();
        let mut proofs_ids = vec![];
        for queues in job.aggregations.chunks(RECURSION_ARITY) {
            let proofs_for_chunk: Vec<_> = (&mut proof_ids_iter).take(queues.len()).collect();
            proofs_ids.push(proofs_for_chunk);
        }

        assert_eq!(
            job.aggregations.chunks(RECURSION_ARITY).len(),
            proofs_ids.len()
        );

        let semaphore = Arc::new(Semaphore::new(max_circuits_in_flight));

        let mut handles = vec![];
        for (circuit_idx, (chunk, proofs_ids_for_chunk)) in job
            .aggregations
            .chunks(RECURSION_ARITY)
            .zip(proofs_ids)
            .enumerate()
        {
            let semaphore = semaphore.clone();

            let object_store = object_store.clone();
            let chunk = Vec::from(chunk);
            let vk = vk.clone();
            let all_leafs_layer_params = job.all_leafs_layer_params.clone();

            let handle = tokio::task::spawn(async move {
                let _permit = semaphore
                    .acquire()
                    .await
                    .expect("failed to get permit to process queues chunk");

                let job_ids: Vec<JobId> = proofs_ids_for_chunk
                    .iter()
                    .map(|id| JobId::new(*id, job.batch_id.chain_id()))
                    .collect();
                let proofs = load_proofs_for_job_ids(&job_ids, &*object_store).await;
                let mut recursive_proofs = vec![];
                for wrapper in proofs {
                    match wrapper {
                        FriProofWrapper::Base(_) => {
                            panic!(
                                "Expected only recursive proofs for node agg {} {}",
                                job.circuit_id, job.batch_id
                            );
                        }
                        FriProofWrapper::Recursive(recursive_proof) => {
                            recursive_proofs.push(recursive_proof)
                        }
                    }
                }

                let (result_circuit_id, recursive_circuit, input_queue) = create_node_witness(
                    &chunk,
                    recursive_proofs,
                    &vk,
                    node_vk_commitment,
                    &all_leafs_layer_params,
                );

                let recursive_circuit_id_and_url = save_recursive_layer_prover_input_artifacts(
                    job.batch_id,
                    circuit_idx,
                    vec![recursive_circuit],
                    AggregationRound::NodeAggregation,
                    job.depth + 1,
                    &*object_store,
                    Some(job.circuit_id),
                )
                .await;

                (
                    (result_circuit_id, input_queue),
                    recursive_circuit_id_and_url,
                )
            });

            handles.push(handle);
        }

        let mut next_aggregations = vec![];
        let mut recursive_circuit_ids_and_urls = vec![];
        for handle in handles {
            let (next_aggregation, recursive_circuit_id_and_url) = handle.await.unwrap();

            next_aggregations.push(next_aggregation);
            recursive_circuit_ids_and_urls.extend(recursive_circuit_id_and_url);
        }

        tracing::info!(
            "Node witness generation for block {} with circuit id {} at depth {} with {} next_aggregations jobs completed in {:?}.",
            job.batch_id,
            job.circuit_id,
            job.depth,
            next_aggregations.len(),
            started_at.elapsed(),
        );

        Ok(NodeAggregationArtifacts {
            circuit_id: job.circuit_id,
            batch_id: job.batch_id,
            depth: job.depth + 1,
            next_aggregations,
            recursive_circuit_ids_and_urls,
        })
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = % metadata.0.batch_id, circuit_id = % metadata.0.circuit_id)
    )]
    async fn prepare_job(
        metadata: Self::Metadata,
        object_store: &dyn ObjectStore,
        keystore: Arc<dyn VerificationKeyManager>,
    ) -> anyhow::Result<NodeAggregationWitnessGeneratorJob> {
        let (metadata, started_at) = metadata;
        let artifacts = Self::get_artifacts(&metadata, object_store).await?;

        WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::NodeAggregation.into()]
            .observe(started_at.elapsed());

        let leaf_vk = keystore
            .load_recursive_layer_verification_key(metadata.circuit_id)
            .context("get_recursive_layer_vk_for_circuit_type")?;
        let node_vk = keystore
            .load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
            )
            .context("get_recursive_layer_vk_for_circuit_type()")?;

        Ok(NodeAggregationWitnessGeneratorJob {
            circuit_id: metadata.circuit_id,
            batch_id: metadata.batch_id,
            depth: metadata.depth,
            aggregations: artifacts.0,
            proofs_ids: metadata.prover_job_ids_for_proofs,
            leaf_vk,
            node_vk,
            all_leafs_layer_params: keystore.get_leaf_vk_params()
                .context("get_leaf_vk_params()")?,
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
            .fri_node_witness_generator_dal()
            .get_next_node_aggregation_job(protocol_version, &pod_name)
            .await
        else {
            return Ok(None);
        };
        let started_at = Instant::now();
        Ok(Some((metadata, started_at)))
    }
}

impl JobMetadata for (NodeAggregationJobMetadata, Instant) {
    fn job_id(&self) -> JobId {
        JobId::new(self.0.id, self.0.batch_id.chain_id())
    }
    fn started_at(&self) -> Instant {
        self.1
    }
}
