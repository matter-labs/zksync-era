use std::{convert::TryInto, sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use circuit_definitions::{
    circuit_definitions::{
        aux_layer::EIP4844VerificationKey, base_layer::ZkSyncBaseProof,
        recursion_layer::ZkSyncRecursionProof,
    },
    eip4844_proof_config,
};
use prover_dal::{Prover, ProverDal};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::{
            field::goldilocks::{GoldilocksExt2, GoldilocksField},
            gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
        },
        circuit_definitions::recursion_layer::{
            scheduler::SchedulerCircuit, ZkSyncRecursionLayerStorageType,
            ZkSyncRecursionLayerVerificationKey, ZkSyncRecursiveLayerCircuit, SCHEDULER_CAPACITY,
        },
        recursion_layer_proof_config,
        zkevm_circuits::scheduler::{input::SchedulerCircuitInstanceWitness, SchedulerConfig},
    },
    get_current_pod_name,
    keys::FriCircuitKey,
    CircuitWrapper, FriProofWrapper,
};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    basic_fri_types::{AggregationRound, FinalProofIds},
    protocol_version::FriProtocolVersionId,
    L1BatchNumber,
};
use zksync_vk_setup_data_server_fri::{keystore::Keystore, utils::get_leaf_vk_params};

use crate::{
    metrics::WITNESS_GENERATOR_METRICS,
    utils::{load_proofs_for_job_ids, SchedulerPartialInputWrapper},
};

pub struct SchedulerArtifacts {
    pub scheduler_circuit: ZkSyncRecursiveLayerCircuit,
}

#[derive(Clone)]
pub struct SchedulerWitnessGeneratorJob {
    block_number: L1BatchNumber,
    scheduler_witness: SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    node_vk: ZkSyncRecursionLayerVerificationKey,
    eip_4844_vk: EIP4844VerificationKey,
}

#[derive(Debug)]
pub struct SchedulerWitnessGenerator {
    config: FriWitnessGeneratorConfig,
    object_store: Arc<dyn ObjectStore>,
    prover_connection_pool: ConnectionPool<Prover>,
    protocol_versions: Vec<FriProtocolVersionId>,
}

impl SchedulerWitnessGenerator {
    pub async fn new(
        config: FriWitnessGeneratorConfig,
        store_factory: &ObjectStoreFactory,
        prover_connection_pool: ConnectionPool<Prover>,
        protocol_versions: Vec<FriProtocolVersionId>,
    ) -> Self {
        Self {
            config,
            object_store: store_factory.create_store().await,
            prover_connection_pool,
            protocol_versions,
        }
    }

    pub fn process_job_sync(
        job: SchedulerWitnessGeneratorJob,
        started_at: Instant,
    ) -> SchedulerArtifacts {
        tracing::info!(
            "Starting fri witness generation of type {:?} for block {}",
            AggregationRound::Scheduler,
            job.block_number.0
        );
        let config = SchedulerConfig {
            proof_config: recursion_layer_proof_config(),
            vk_fixed_parameters: job.node_vk.into_inner().fixed_parameters,
            capacity: SCHEDULER_CAPACITY,
            _marker: std::marker::PhantomData,
        };

        let eip_4844_config = eip4844_proof_config();

        let scheduler_circuit = SchedulerCircuit {
            witness: job.scheduler_witness,
            config,
            transcript_params: (),
            eip4844_proof_config: Some(eip_4844_config),
            eip4844_vk: Some(job.eip_4844_vk.clone()),
            eip4844_vk_fixed_parameters: Some(job.eip_4844_vk.fixed_parameters),
            _marker: std::marker::PhantomData,
        };
        WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::Scheduler.into()]
            .observe(started_at.elapsed());

        tracing::info!(
            "Scheduler generation for block {} is complete in {:?}",
            job.block_number.0,
            started_at.elapsed()
        );

        SchedulerArtifacts {
            scheduler_circuit: ZkSyncRecursiveLayerCircuit::SchedulerCircuit(scheduler_circuit),
        }
    }
}

#[async_trait]
impl JobProcessor for SchedulerWitnessGenerator {
    type Job = SchedulerWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    type JobArtifacts = SchedulerArtifacts;

    const SERVICE_NAME: &'static str = "fri_scheduler_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.connection().await.unwrap();
        let pod_name = get_current_pod_name();
        let Some(l1_batch_number) = prover_connection
            .fri_witness_generator_dal()
            .get_next_scheduler_witness_job(&self.protocol_versions, &pod_name)
            .await
        else {
            return Ok(None);
        };
        let proof_job_ids = prover_connection
            .fri_scheduler_dependency_tracker_dal()
            .get_final_prover_job_ids_for(l1_batch_number)
            .await;

        Ok(Some((
            l1_batch_number,
            prepare_job(l1_batch_number, proof_job_ids, &*self.object_store)
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
            .mark_scheduler_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: SchedulerWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<SchedulerArtifacts>> {
        tokio::task::spawn_blocking(move || {
            let block_number = job.block_number;
            let _span = tracing::info_span!("scheduler", %block_number).entered();
            Ok(Self::process_job_sync(job, started_at))
        })
    }

    async fn save_result(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        artifacts: SchedulerArtifacts,
    ) -> anyhow::Result<()> {
        let key = FriCircuitKey {
            block_number: job_id,
            circuit_id: 1,
            sequence_number: 0,
            depth: 0,
            aggregation_round: AggregationRound::Scheduler,
        };
        let blob_save_started_at = Instant::now();
        let scheduler_circuit_blob_url = self
            .object_store
            .put(key, &CircuitWrapper::Recursive(artifacts.scheduler_circuit))
            .await
            .unwrap();
        WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::Scheduler.into()]
            .observe(blob_save_started_at.elapsed());

        let mut prover_connection = self.prover_connection_pool.connection().await.unwrap();
        let mut transaction = prover_connection.start_transaction().await.unwrap();
        let protocol_version_id = transaction
            .fri_witness_generator_dal()
            .protocol_version_for_l1_batch(job_id)
            .await;
        transaction
            .fri_prover_jobs_dal()
            .insert_prover_job(
                job_id,
                1,
                0,
                0,
                AggregationRound::Scheduler,
                &scheduler_circuit_blob_url,
                false,
                protocol_version_id,
            )
            .await;

        transaction
            .fri_witness_generator_dal()
            .mark_scheduler_job_as_successful(job_id, started_at.elapsed())
            .await;

        transaction.commit().await.unwrap();
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
            .context("failed to acquire DB connection for SchedulerWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_scheduler_witness_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for SchedulerWitnessGenerator")
    }
}

pub async fn prepare_job(
    l1_batch_number: L1BatchNumber,
    proof_job_ids: FinalProofIds,
    object_store: &dyn ObjectStore,
) -> anyhow::Result<SchedulerWitnessGeneratorJob> {
    let started_at = Instant::now();
    let proofs = load_proofs_for_job_ids(&proof_job_ids.node_proof_ids, object_store).await;
    WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::Scheduler.into()]
        .observe(started_at.elapsed());

    let recursive_proofs: Result<Vec<ZkSyncRecursionProof>, anyhow::Error> = proofs.into_iter().map(|wrapper| {
        match wrapper {
            FriProofWrapper::Base(_) => Err(anyhow::anyhow!(
                "Expected only recursive proofs for scheduler l1 batch {l1_batch_number}, got Base"
            )),
            FriProofWrapper::Recursive(recursive_proof) => {
                Ok(recursive_proof.into_inner())
            }
            FriProofWrapper::Eip4844(_) => {
                Err(anyhow::anyhow!("Expected only recursive proofs for  scheduler l1 batch {l1_batch_number}, got EIP4844"))
            }
        }
    }).collect();
    let recursive_proofs = recursive_proofs?;

    let proofs = load_proofs_for_job_ids(&proof_job_ids.eip_4844_proof_ids, object_store).await;

    let eip_4844_proofs: Result<Vec<ZkSyncBaseProof>, anyhow::Error> = proofs
        .into_iter()
        .map(|wrapper| match wrapper {
            FriProofWrapper::Base(_) => Err(anyhow::anyhow!(
                "Expected only EIP4844 proofs for scheduler l1 batch {l1_batch_number}, got Base"
            )),
            FriProofWrapper::Recursive(_) => Err(anyhow::anyhow!(
            "Expected only EIP4844 proofs for scheduler l1 batch {l1_batch_number}, got Recursive"
        )),
            FriProofWrapper::Eip4844(eip_4844_proof) => Ok(eip_4844_proof),
        })
        .collect();
    let eip_4844_proofs = eip_4844_proofs?;

    let started_at = Instant::now();
    let keystore = Keystore::default();
    let node_vk = keystore
        .load_recursive_layer_verification_key(
            ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
        )
        .context("get_recursive_layer_vk_for_circuit_type()")?;
    let eip_4844_vk = keystore
        .load_4844_verification_key()
        .context("get_eip_4844_vk")?;
    let SchedulerPartialInputWrapper(mut scheduler_witness) =
        object_store.get(l1_batch_number).await.unwrap();
    scheduler_witness.node_layer_vk_witness = node_vk.clone().into_inner();

    scheduler_witness.proof_witnesses = recursive_proofs.into();
    scheduler_witness.eip4844_proofs = eip_4844_proofs.into();

    let leaf_vk_commits = get_leaf_vk_params(&keystore).context("get_leaf_vk_params()")?;
    let leaf_layer_params = leaf_vk_commits
        .iter()
        .map(|el| el.1.clone())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    scheduler_witness.leaf_layer_parameters = leaf_layer_params;

    WITNESS_GENERATOR_METRICS.prepare_job_time[&AggregationRound::Scheduler.into()]
        .observe(started_at.elapsed());

    Ok(SchedulerWitnessGeneratorJob {
        block_number: l1_batch_number,
        scheduler_witness,
        node_vk,
        eip_4844_vk,
    })
}
