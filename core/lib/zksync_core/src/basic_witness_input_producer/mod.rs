use anyhow::Context;
use std::sync::Arc;
use std::time::Instant;

use zksync_dal::{basic_witness_input_producer_dal::JOB_MAX_ATTEMPT, ConnectionPool};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::witness_block_state::WitnessBlockState;
use zksync_types::{L1BatchNumber, L2ChainId};

use async_trait::async_trait;
use multivm::interface::{L2BlockEnv, VmInterface};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

mod metrics;
mod vm_interactions;

use self::metrics::METRICS;
use self::vm_interactions::{create_vm, execute_tx};

/// Component that extracts all data (from DB) necessary to run a Basic Witness Generator.
/// Does this by rerunning an entire L1Batch and extracting information from both the VM run and DB.
/// This component will upload Witness Inputs to the object store.
/// This allows Witness Generator workflow (that needs only Basic Witness Generator Inputs)
/// to be run only using the object store information, having no other external dependency.
#[derive(Debug)]
pub struct BasicWitnessInputProducer {
    connection_pool: ConnectionPool,
    l2_chain_id: L2ChainId,
    object_store: Arc<dyn ObjectStore>,
}

impl BasicWitnessInputProducer {
    pub async fn new(
        connection_pool: ConnectionPool,
        store_factory: &ObjectStoreFactory,
        l2_chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        Ok(BasicWitnessInputProducer {
            connection_pool,
            object_store: store_factory.create_store().await.into(),
            l2_chain_id,
        })
    }

    fn process_job_impl(
        rt_handle: Handle,
        l1_batch_number: L1BatchNumber,
        started_at: Instant,
        connection_pool: ConnectionPool,
        l2_chain_id: L2ChainId,
    ) -> anyhow::Result<WitnessBlockState> {
        let mut connection = rt_handle
            .block_on(connection_pool.access_storage())
            .context("failed to get connection for BasicWitnessInputProducer")?;

        let miniblocks_execution_data = rt_handle.block_on(
            connection
                .transactions_dal()
                .get_miniblocks_to_execute_for_l1_batch(l1_batch_number),
        )?;

        let (mut vm, storage_view) =
            create_vm(rt_handle.clone(), l1_batch_number, connection, l2_chain_id)
                .context("failed to create vm for BasicWitnessInputProducer")?;

        tracing::info!("Started execution of l1_batch: {l1_batch_number:?}");

        let next_miniblocks_data = miniblocks_execution_data
            .iter()
            .skip(1)
            .map(Some)
            .chain([None]);
        let miniblocks_data = miniblocks_execution_data.iter().zip(next_miniblocks_data);

        for (miniblock_data, next_miniblock_data) in miniblocks_data {
            tracing::debug!(
                "Started execution of miniblock: {:?}, executing {:?} transactions",
                miniblock_data.number,
                miniblock_data.txs.len(),
            );
            for tx in &miniblock_data.txs {
                tracing::trace!("Started execution of tx: {tx:?}");
                execute_tx(tx, &mut vm)
                    .context("failed to execute transaction in BasicWitnessInputProducer")?;
                tracing::trace!("Finished execution of tx: {tx:?}");
            }
            if let Some(next_miniblock_data) = next_miniblock_data {
                vm.start_new_l2_block(L2BlockEnv::from_miniblock_data(next_miniblock_data));
            }

            tracing::debug!(
                "Finished execution of miniblock: {:?}",
                miniblock_data.number
            );
        }
        vm.finish_batch();
        tracing::info!("Finished execution of l1_batch: {l1_batch_number:?}");

        METRICS.process_batch_time.observe(started_at.elapsed());
        tracing::info!(
            "BasicWitnessInputProducer took {:?} for L1BatchNumber {}",
            started_at.elapsed(),
            l1_batch_number.0
        );

        let witness_block_state = (*storage_view).borrow().witness_block_state();
        Ok(witness_block_state)
    }
}

#[async_trait]
impl JobProcessor for BasicWitnessInputProducer {
    type Job = L1BatchNumber;
    type JobId = L1BatchNumber;
    type JobArtifacts = WitnessBlockState;
    const SERVICE_NAME: &'static str = "basic_witness_input_producer";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut connection = self.connection_pool.access_storage().await?;
        let l1_batch_to_process = connection
            .basic_witness_input_producer_dal()
            .get_next_basic_witness_input_producer_job()
            .await
            .context("failed to get next basic witness input producer job")?;
        Ok(l1_batch_to_process.map(|number| (number, number)))
    }

    async fn save_failure(&self, job_id: Self::JobId, started_at: Instant, error: String) {
        let attempts = self
            .connection_pool
            .access_storage()
            .await
            .unwrap()
            .basic_witness_input_producer_dal()
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
        job: Self::Job,
        started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        let l2_chain_id = self.l2_chain_id;
        let connection_pool = self.connection_pool.clone();
        tokio::task::spawn_blocking(move || {
            let rt_handle = Handle::current();
            Self::process_job_impl(
                rt_handle,
                job,
                started_at,
                connection_pool.clone(),
                l2_chain_id,
            )
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
            .context("failed to upload artifacts for BasicWitnessInputProducer")?;
        METRICS
            .upload_input_time
            .observe(upload_started_at.elapsed());
        let mut connection = self
            .connection_pool
            .access_storage()
            .await
            .context("failed to acquire DB connection for BasicWitnessInputProducer")?;
        let mut transaction = connection
            .start_transaction()
            .await
            .context("failed to acquire DB transaction for BasicWitnessInputProducer")?;
        transaction
            .basic_witness_input_producer_dal()
            .mark_job_as_successful(job_id, started_at, &object_path)
            .await
            .context("failed to mark job as successful for BasicWitnessInputProducer")?;
        transaction
            .witness_generator_dal()
            .mark_witness_inputs_job_as_queued(job_id)
            .await;
        transaction
            .commit()
            .await
            .context("failed to commit DB transaction for BasicWitnessInputProducer")?;
        METRICS.block_number_processed.set(job_id.0 as i64);
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        JOB_MAX_ATTEMPT as u32
    }

    async fn get_job_attempts(&self, job_id: &L1BatchNumber) -> anyhow::Result<u32> {
        let mut connection = self
            .connection_pool
            .access_storage()
            .await
            .context("failed to acquire DB connection for BasicWitnessInputProducer")?;
        connection
            .basic_witness_input_producer_dal()
            .get_basic_witness_input_producer_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for BasicWitnessInputProducer")
    }
}
