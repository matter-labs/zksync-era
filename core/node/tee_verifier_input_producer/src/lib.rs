//! Produces input for a TEE Verifier
//!
//! Extract all data needed to re-execute and verify an L1Batch without accessing
//! the DB and/or the object store.
//!
//! For testing purposes, the L1 batch is re-executed immediately for now.
//! Eventually, this component will only extract the inputs and send them to another
//! machine over a "to be defined" channel, e.g., save them to an object store.

use std::{sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use tokio::task::JoinHandle;
use zksync_dal::{tee_verifier_input_producer_dal::JOB_MAX_ATTEMPT, ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::inputs::{
    PrepareBasicCircuitsJob, TeeVerifierInput, V1TeeVerifierInput,
};
use zksync_queued_job_processor::JobProcessor;
use zksync_tee_verifier::Verify;
use zksync_types::{L1BatchNumber, L2ChainId};
use zksync_utils::u256_to_h256;
use zksync_vm_utils::storage::L1BatchParamsProvider;

use self::metrics::METRICS;

mod metrics;

/// Component that extracts all data (from DB) necessary to run a TEE Verifier.
#[derive(Debug)]
pub struct TeeVerifierInputProducer {
    connection_pool: ConnectionPool<Core>,
    l2_chain_id: L2ChainId,
    object_store: Arc<dyn ObjectStore>,
}

impl TeeVerifierInputProducer {
    pub async fn new(
        connection_pool: ConnectionPool<Core>,
        object_store: Arc<dyn ObjectStore>,
        l2_chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        Ok(TeeVerifierInputProducer {
            connection_pool,
            object_store,
            l2_chain_id,
        })
    }

    async fn process_job_impl(
        l1_batch_number: L1BatchNumber,
        started_at: Instant,
        connection_pool: ConnectionPool<Core>,
        object_store: Arc<dyn ObjectStore>,
        l2_chain_id: L2ChainId,
    ) -> anyhow::Result<TeeVerifierInput> {
        let prepare_basic_circuits_job: PrepareBasicCircuitsJob = object_store
            .get(l1_batch_number)
            .await
            .context("failed to get PrepareBasicCircuitsJob from object store")?;

        let mut connection = connection_pool
            .connection()
            .await
            .context("failed to get connection for TeeVerifierInputProducer")?;

        let l2_blocks_execution_data = connection
            .transactions_dal()
            .get_l2_blocks_to_execute_for_l1_batch(l1_batch_number)
            .await?;

        let l1_batch_header = connection
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .with_context(|| format!("header is missing for L1 batch #{l1_batch_number}"))?
            .unwrap();

        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut connection)
            .await
            .context("failed initializing L1 batch params provider")?;

        let first_miniblock_in_batch = l1_batch_params_provider
            .load_first_l2_block_in_batch(&mut connection, l1_batch_number)
            .await
            .with_context(|| {
                format!("failed loading first miniblock in L1 batch #{l1_batch_number}")
            })?
            .with_context(|| format!("no miniblocks persisted for L1 batch #{l1_batch_number}"))?;

        // In the state keeper, this value is used to reject execution.
        // All batches have already been executed by State Keeper.
        // This means we don't want to reject any execution, therefore we're using MAX as an allow all.
        let validation_computational_gas_limit = u32::MAX;

        let (system_env, l1_batch_env) = l1_batch_params_provider
            .load_l1_batch_params(
                &mut connection,
                &first_miniblock_in_batch,
                validation_computational_gas_limit,
                l2_chain_id,
            )
            .await
            .context("expected miniblock to be executed and sealed")?;

        let used_contract_hashes = l1_batch_header
            .used_contract_hashes
            .into_iter()
            .map(u256_to_h256)
            .collect();

        // `get_factory_deps()` returns the bytecode in chunks of `Vec<[u8; 32]>`,
        // but `fn store_factory_dep(&mut self, hash: H256, bytecode: Vec<u8>)` in `InMemoryStorage` wants flat byte vecs.
        pub fn into_flattened<T: Clone, const N: usize>(data: Vec<[T; N]>) -> Vec<T> {
            let mut new = Vec::new();
            for slice in data.iter() {
                new.extend_from_slice(slice);
            }
            new
        }

        let used_contracts = connection
            .factory_deps_dal()
            .get_factory_deps(&used_contract_hashes)
            .await
            .into_iter()
            .map(|(hash, bytes)| (u256_to_h256(hash), into_flattened(bytes)))
            .collect();

        tracing::info!("Started execution of l1_batch: {l1_batch_number:?}");

        let tee_verifier_input = V1TeeVerifierInput::new(
            prepare_basic_circuits_job,
            l2_blocks_execution_data,
            l1_batch_env,
            system_env,
            used_contracts,
        );

        // TODO (SEC-263): remove these 2 lines after successful testnet runs
        tee_verifier_input.clone().verify()?;
        tracing::info!("Looks like we verified {l1_batch_number} correctly");

        tracing::info!("Finished execution of l1_batch: {l1_batch_number:?}");

        METRICS.process_batch_time.observe(started_at.elapsed());
        tracing::debug!(
            "TeeVerifierInputProducer took {:?} for L1BatchNumber {}",
            started_at.elapsed(),
            l1_batch_number.0
        );

        Ok(TeeVerifierInput::new(tee_verifier_input))
    }
}

#[async_trait]
impl JobProcessor for TeeVerifierInputProducer {
    type Job = L1BatchNumber;
    type JobId = L1BatchNumber;
    type JobArtifacts = TeeVerifierInput;
    const SERVICE_NAME: &'static str = "tee_verifier_input_producer";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut connection = self.connection_pool.connection().await?;
        let l1_batch_to_process = connection
            .tee_verifier_input_producer_dal()
            .get_next_tee_verifier_input_producer_job()
            .await
            .context("failed to get next basic witness input producer job")?;
        Ok(l1_batch_to_process.map(|number| (number, number)))
    }

    async fn save_failure(&self, job_id: Self::JobId, started_at: Instant, error: String) {
        let attempts = self
            .connection_pool
            .connection()
            .await
            .unwrap()
            .tee_verifier_input_producer_dal()
            .mark_job_as_failed(job_id, started_at, error)
            .await
            .expect("errored whilst marking job as failed");
        if let Some(tries) = attempts {
            tracing::warn!("Failed to process job: {job_id:?}, after {tries} tries.");
        } else {
            tracing::warn!("L1 Batch {job_id:?} was processed successfully by another worker.");
        }
    }

    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: Self::Job,
        started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        let l2_chain_id = self.l2_chain_id;
        let connection_pool = self.connection_pool.clone();
        let object_store = self.object_store.clone();
        tokio::task::spawn(async move {
            Self::process_job_impl(
                job,
                started_at,
                connection_pool.clone(),
                object_store,
                l2_chain_id,
            )
            .await
        })
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
    ) -> anyhow::Result<()> {
        let upload_started_at = Instant::now();
        let object_path = self
            .object_store
            .put(job_id, &artifacts)
            .await
            .context("failed to upload artifacts for TeeVerifierInputProducer")?;
        METRICS
            .upload_input_time
            .observe(upload_started_at.elapsed());
        let mut connection = self
            .connection_pool
            .connection()
            .await
            .context("failed to acquire DB connection for TeeVerifierInputProducer")?;
        let mut transaction = connection
            .start_transaction()
            .await
            .context("failed to acquire DB transaction for TeeVerifierInputProducer")?;
        transaction
            .tee_verifier_input_producer_dal()
            .mark_job_as_successful(job_id, started_at, &object_path)
            .await
            .context("failed to mark job as successful for TeeVerifierInputProducer")?;
        transaction
            .tee_proof_generation_dal()
            .insert_tee_proof_generation_job(job_id)
            .await?;
        transaction
            .commit()
            .await
            .context("failed to commit DB transaction for TeeVerifierInputProducer")?;
        METRICS.block_number_processed.set(job_id.0 as i64);
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        JOB_MAX_ATTEMPT as u32
    }

    async fn get_job_attempts(&self, job_id: &L1BatchNumber) -> anyhow::Result<u32> {
        let mut connection = self
            .connection_pool
            .connection()
            .await
            .context("failed to acquire DB connection for TeeVerifierInputProducer")?;
        connection
            .tee_verifier_input_producer_dal()
            .get_tee_verifier_input_producer_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for TeeVerifierInputProducer")
    }
}
