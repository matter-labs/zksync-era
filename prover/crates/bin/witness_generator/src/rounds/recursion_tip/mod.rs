use std::{sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use circuit_definitions::{
    circuit_definitions::recursion_layer::{
        recursion_tip::RecursionTipCircuit, ZkSyncRecursionLayerStorageType,
        ZkSyncRecursionLayerVerificationKey, ZkSyncRecursiveLayerCircuit, RECURSION_TIP_ARITY,
    },
    recursion_layer_proof_config,
};
use zkevm_test_harness::{
    boojum::{
        field::{
            goldilocks::{GoldilocksExt2, GoldilocksField},
            Field, U64Representable,
        },
        gadgets::{
            queue::QueueState, recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
            traits::allocatable::CSAllocatable,
        },
    },
    witness::{
        recursive_aggregation::compute_node_vk_commitment,
        utils::take_sponge_like_queue_state_from_simulator,
    },
    zkevm_circuits::{
        recursion::{
            leaf_layer::input::RecursionLeafParametersWitness,
            recursion_tip::{
                input::{RecursionTipInputWitness, RecursionTipInstanceWitness},
                RecursionTipConfig,
            },
        },
        scheduler::aux::BaseLayerCircuitType,
    },
};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{get_current_pod_name, keys::ClosedFormInputKey};
use zksync_prover_keystore::{keystore::Keystore, utils::get_leaf_vk_params};
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchId,
};

use super::JobMetadata;
use crate::{
    artifacts::{ArtifactsManager, JobId},
    metrics::WITNESS_GENERATOR_METRICS,
    rounds::JobManager,
    utils::ClosedFormInputWrapper,
};

mod artifacts;

#[derive(Clone)]
pub struct RecursionTipWitnessGeneratorJob {
    batch_id: L1BatchId,
    recursion_tip_witness: RecursionTipInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    node_vk: ZkSyncRecursionLayerVerificationKey,
}

#[derive(Clone)]
pub struct RecursionTipArtifacts {
    pub recursion_tip_circuit: ZkSyncRecursiveLayerCircuit,
}

#[derive(Clone)]
pub struct RecursionTipJobMetadata {
    pub batch_id: L1BatchId,
    pub final_node_proof_job_ids: Vec<(u8, JobId)>,
}

pub struct RecursionTip;

#[async_trait]
impl JobManager for RecursionTip {
    type Job = RecursionTipWitnessGeneratorJob;
    type Metadata = RecursionTipJobMetadata;

    const ROUND: AggregationRound = AggregationRound::RecursionTip;
    const SERVICE_NAME: &'static str = "recursion_tip_witness_generator";

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %job.batch_id)
    )]
    async fn process_job(
        job: Self::Job,
        _object_store: Arc<dyn ObjectStore>,
        _max_circuits_in_flight: usize,
        started_at: Instant,
    ) -> anyhow::Result<RecursionTipArtifacts> {
        tracing::info!(
            "Starting fri witness generation of type {:?} for block {}",
            AggregationRound::RecursionTip,
            job.batch_id
        );
        let config = RecursionTipConfig {
            proof_config: recursion_layer_proof_config(),
            vk_fixed_parameters: job.node_vk.clone().into_inner().fixed_parameters,
            _marker: std::marker::PhantomData,
        };

        let recursive_tip_circuit = RecursionTipCircuit {
            witness: job.recursion_tip_witness,
            config,
            transcript_params: (),
            _marker: std::marker::PhantomData,
        };

        WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::RecursionTip.into()]
            .observe(started_at.elapsed());

        tracing::info!(
            "Recursion tip generation for block {} is complete in {:?}",
            job.batch_id,
            started_at.elapsed()
        );

        Ok(RecursionTipArtifacts {
            recursion_tip_circuit: ZkSyncRecursiveLayerCircuit::RecursionTipCircuit(
                recursive_tip_circuit,
            ),
        })
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %metadata.batch_id)
    )]
    async fn prepare_job(
        metadata: RecursionTipJobMetadata,
        object_store: &dyn ObjectStore,
        keystore: Keystore,
    ) -> anyhow::Result<RecursionTipWitnessGeneratorJob> {
        let started_at = Instant::now();
        let recursion_tip_proofs =
            Self::get_artifacts(&metadata.final_node_proof_job_ids, object_store).await?;
        WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::RecursionTip.into()]
            .observe(started_at.elapsed());

        let node_vk = keystore
            .load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
            )
            .context("get_recursive_layer_vk_for_circuit_type()")?;

        let node_layer_vk_commitment = compute_node_vk_commitment(node_vk.clone());

        let mut recursion_queues = vec![];
        for circuit_id in BaseLayerCircuitType::as_iter_u8() {
            let key = ClosedFormInputKey {
                batch_id: metadata.batch_id,
                circuit_id,
            };
            let ClosedFormInputWrapper(_, recursion_queue) = object_store.get(key).await?;
            recursion_queues.push((circuit_id, recursion_queue));
        }

        // RECURSION_TIP_ARITY is the maximum amount of proof that a single recursion tip can support.
        // Given recursion_tip has at most 1 proof per circuit, it implies we can't add more circuit types without bumping arity up.
        assert!(
            RECURSION_TIP_ARITY >= recursion_queues.len(),
            "recursion tip received more circuits ({}) than supported ({})",
            recursion_queues.len(),
            RECURSION_TIP_ARITY
        );
        let mut branch_circuit_type_set = [GoldilocksField::ZERO; RECURSION_TIP_ARITY];
        let mut queue_set: [_; RECURSION_TIP_ARITY] =
            std::array::from_fn(|_| QueueState::placeholder_witness());

        for (index, (circuit_id, recursion_queue)) in recursion_queues.iter().enumerate() {
            branch_circuit_type_set[index] =
                GoldilocksField::from_u64_unchecked(*circuit_id as u64);
            queue_set[index] = take_sponge_like_queue_state_from_simulator(recursion_queue);
        }

        const EXPECTED_RECURSION_TIP_LEAVES: usize = 20;

        let leaf_vk_commits = get_leaf_vk_params(&keystore).context("get_leaf_vk_params()")?;
        assert_eq!(
            leaf_vk_commits.len(),
            EXPECTED_RECURSION_TIP_LEAVES,
            "expected 16 leaf vk commits, which corresponds to the numebr of circuits, got {}",
            leaf_vk_commits.len()
        );
        let leaf_layer_parameters: [RecursionLeafParametersWitness<GoldilocksField>;
            EXPECTED_RECURSION_TIP_LEAVES] = leaf_vk_commits
            .iter()
            .map(|el| el.1.clone())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        let input = RecursionTipInputWitness {
            leaf_layer_parameters,
            node_layer_vk_commitment,
            branch_circuit_type_set,
            queue_set,
        };

        let recursion_tip_witness = RecursionTipInstanceWitness {
            input,
            vk_witness: node_vk.clone().into_inner(),
            proof_witnesses: recursion_tip_proofs.into(),
        };

        WITNESS_GENERATOR_METRICS.prepare_job_time[&AggregationRound::RecursionTip.into()]
            .observe(started_at.elapsed());

        Ok(RecursionTipWitnessGeneratorJob {
            batch_id: metadata.batch_id,
            recursion_tip_witness,
            node_vk,
        })
    }

    async fn get_metadata(
        connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
    ) -> anyhow::Result<Option<Self::Metadata>> {
        let pod_name = get_current_pod_name();
        let Some((l1_batch_id, number_of_final_node_jobs)) = connection_pool
            .connection()
            .await?
            .fri_recursion_tip_witness_generator_dal()
            .get_next_recursion_tip_witness_job(protocol_version, &pod_name)
            .await
        else {
            return Ok(None);
        };

        let final_node_proof_job_ids: Vec<(u8, JobId)> = connection_pool
            .connection()
            .await?
            .fri_prover_jobs_dal()
            .get_final_node_proof_job_ids_for(l1_batch_id)
            .await
            .into_iter()
            .map(|(circuit_id, id, chain_id)| (circuit_id, JobId::new(id, chain_id)))
            .collect();

        assert_eq!(
            final_node_proof_job_ids.len(),
            number_of_final_node_jobs as usize,
            "recursion tip witness job was scheduled without all final node jobs being completed; expected {}, got {}",
            number_of_final_node_jobs, final_node_proof_job_ids.len()
        );

        Ok(Some(RecursionTipJobMetadata {
            batch_id: l1_batch_id,
            final_node_proof_job_ids,
        }))
    }
}

impl JobMetadata for RecursionTipJobMetadata {
    fn job_id(&self) -> JobId {
        JobId::new(self.batch_id.batch_number().0, self.batch_id.chain_id())
    }
}
