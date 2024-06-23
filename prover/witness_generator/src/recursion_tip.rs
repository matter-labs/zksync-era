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
use prover_dal::{Prover, ProverDal};
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
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStore;
use zksync_prover_fri_types::{
    get_current_pod_name,
    keys::{ClosedFormInputKey, FriCircuitKey},
    CircuitWrapper,
};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchNumber,
};
use zksync_vk_setup_data_server_fri::{keystore::Keystore, utils::get_leaf_vk_params};

use crate::{
    metrics::WITNESS_GENERATOR_METRICS,
    utils::{load_proofs_for_recursion_tip, ClosedFormInputWrapper},
};

#[derive(Clone)]
pub struct RecursionTipWitnessGeneratorJob {
    block_number: L1BatchNumber,
    recursion_tip_witness: RecursionTipInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    node_vk: ZkSyncRecursionLayerVerificationKey,
}

pub struct RecursionTipArtifacts {
    pub recursion_tip_circuit: ZkSyncRecursiveLayerCircuit,
}

#[derive(Debug)]
pub struct RecursionTipWitnessGenerator {
    config: FriWitnessGeneratorConfig,
    object_store: Arc<dyn ObjectStore>,
    prover_connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
}

impl RecursionTipWitnessGenerator {
    pub fn new(
        config: FriWitnessGeneratorConfig,
        object_store: Arc<dyn ObjectStore>,
        prover_connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
    ) -> Self {
        Self {
            config,
            object_store,
            prover_connection_pool,
            protocol_version,
        }
    }

    pub fn process_job_sync(
        job: RecursionTipWitnessGeneratorJob,
        started_at: Instant,
    ) -> RecursionTipArtifacts {
        tracing::info!(
            "Starting fri witness generation of type {:?} for block {}",
            AggregationRound::RecursionTip,
            job.block_number.0
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
            job.block_number.0,
            started_at.elapsed()
        );

        RecursionTipArtifacts {
            recursion_tip_circuit: ZkSyncRecursiveLayerCircuit::RecursionTipCircuit(
                recursive_tip_circuit,
            ),
        }
    }
}

#[async_trait]
impl JobProcessor for RecursionTipWitnessGenerator {
    type Job = RecursionTipWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    type JobArtifacts = RecursionTipArtifacts;

    const SERVICE_NAME: &'static str = "recursion_tip_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.connection().await?;
        let pod_name = get_current_pod_name();
        let Some((l1_batch_number, number_of_final_node_jobs)) = prover_connection
            .fri_witness_generator_dal()
            .get_next_recursion_tip_witness_job(self.protocol_version, &pod_name)
            .await
        else {
            return Ok(None);
        };

        let final_node_proof_job_ids = prover_connection
            .fri_prover_jobs_dal()
            .get_final_node_proof_job_ids_for(l1_batch_number)
            .await;

        assert_eq!(
            final_node_proof_job_ids.len(),
            number_of_final_node_jobs as usize,
            "recursion tip witness job was scheduled without all final node jobs being completed; expected {}, got {}", 
            number_of_final_node_jobs, final_node_proof_job_ids.len()
        );

        Ok(Some((
            l1_batch_number,
            prepare_job(
                l1_batch_number,
                final_node_proof_job_ids,
                &*self.object_store,
            )
            .await
            .context("prepare_job()")?,
        )))
    }

    async fn save_failure(&self, job_id: L1BatchNumber, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_recursion_tip_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: RecursionTipWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<RecursionTipArtifacts>> {
        tokio::task::spawn_blocking(move || {
            let block_number = job.block_number;
            let _span = tracing::info_span!("recursion_tip", %block_number).entered();
            Ok(Self::process_job_sync(job, started_at))
        })
    }

    async fn save_result(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        artifacts: RecursionTipArtifacts,
    ) -> anyhow::Result<()> {
        let key = FriCircuitKey {
            block_number: job_id,
            circuit_id: 255,
            sequence_number: 0,
            depth: 0,
            aggregation_round: AggregationRound::RecursionTip,
        };
        let blob_save_started_at = Instant::now();

        let recursion_tip_circuit_blob_url = self
            .object_store
            .put(
                key,
                &CircuitWrapper::Recursive(artifacts.recursion_tip_circuit),
            )
            .await?;

        WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::RecursionTip.into()]
            .observe(blob_save_started_at.elapsed());

        let mut prover_connection = self.prover_connection_pool.connection().await?;
        let mut transaction = prover_connection.start_transaction().await?;
        let protocol_version_id = transaction
            .fri_witness_generator_dal()
            .protocol_version_for_l1_batch(job_id)
            .await;
        transaction
            .fri_prover_jobs_dal()
            .insert_prover_job(
                job_id,
                ZkSyncRecursionLayerStorageType::RecursionTipCircuit as u8,
                0,
                0,
                AggregationRound::RecursionTip,
                &recursion_tip_circuit_blob_url,
                false,
                protocol_version_id,
            )
            .await;

        transaction
            .fri_witness_generator_dal()
            .mark_recursion_tip_job_as_successful(job_id, started_at.elapsed())
            .await;

        transaction.commit().await?;
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &L1BatchNumber) -> anyhow::Result<u32> {
        let mut prover_storage = self
            .prover_connection_pool
            .connection()
            .await
            .context("failed to acquire DB connection for RecursionTipWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_recursion_tip_witness_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for RecursionTipWitnessGenerator")
    }
}

pub async fn prepare_job(
    l1_batch_number: L1BatchNumber,
    final_node_proof_job_ids: Vec<(u8, u32)>,
    object_store: &dyn ObjectStore,
) -> anyhow::Result<RecursionTipWitnessGeneratorJob> {
    let started_at = Instant::now();
    let recursion_tip_proofs =
        load_proofs_for_recursion_tip(final_node_proof_job_ids, object_store).await?;
    WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::RecursionTip.into()]
        .observe(started_at.elapsed());

    let keystore = Keystore::default();
    let node_vk = keystore
        .load_recursive_layer_verification_key(
            ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
        )
        .context("get_recursive_layer_vk_for_circuit_type()")?;

    let node_layer_vk_commitment = compute_node_vk_commitment(node_vk.clone());

    let mut recursion_queues = vec![];
    for circuit_id in BaseLayerCircuitType::as_iter_u8() {
        let key = ClosedFormInputKey {
            block_number: l1_batch_number,
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
        branch_circuit_type_set[index] = GoldilocksField::from_u64_unchecked(*circuit_id as u64);
        queue_set[index] = take_sponge_like_queue_state_from_simulator(recursion_queue);
    }

    let leaf_vk_commits = get_leaf_vk_params(&keystore).context("get_leaf_vk_params()")?;
    assert_eq!(
        leaf_vk_commits.len(),
        16,
        "expected 16 leaf vk commits, which corresponds to the numebr of circuits, got {}",
        leaf_vk_commits.len()
    );
    let leaf_layer_parameters: [RecursionLeafParametersWitness<GoldilocksField>; 16] =
        leaf_vk_commits
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
        block_number: l1_batch_number,
        recursion_tip_witness,
        node_vk,
    })
}
