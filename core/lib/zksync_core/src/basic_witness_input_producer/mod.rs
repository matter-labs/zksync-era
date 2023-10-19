use std::sync::Arc;
use std::time::Instant;

use zksync_config::configs::chain::{NetworkConfig, StateKeeperConfig};
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::witness_block_state::WitnessBlockState;
use zksync_types::{L1BatchNumber, L2ChainId};

use async_trait::async_trait;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

mod vm_interactions;

use vm_interactions::{create_vm, execute_tx, get_miniblock_transition_state};

#[derive(Debug)]
pub struct BasicWitnessInputProducerJob {
    l1_batch_number: L1BatchNumber,
}

pub struct BasicWitnessInputProducer {
    connection_pool: ConnectionPool,
    validation_computational_gas_limit: u32,
    l2_chain_id: L2ChainId,
    object_store: Arc<dyn ObjectStore>,
}

impl BasicWitnessInputProducer {
    pub async fn new(
        connection_pool: ConnectionPool,
        store_factory: &ObjectStoreFactory,
        state_keeper_config: &StateKeeperConfig,
        network_config: &NetworkConfig,
    ) -> anyhow::Result<Self> {
        // BEWARE, HERE BE DRAGONS.
        // This usage here is a race condition waiting to happen.
        // Context -- The VM run in BasicWitnessInputProducer MUST be identical to run from StateKeeper.
        // Race condition scenario:
        // 1. StateKeeper runs with a config A, which has value a for `validation_computational_gas_limit`
        // 2. StateKeeper runs the batch and saves data to database.
        // 3. Deployment is made and changes to config B, with value b for `validation_computational_gas_limit`.
        // 4. BasicWitnessInputProducer loads config B
        // 5. BasicWitnessInputProducer runs the L1Batch that was saved with config A using config B.
        // In this scenario, the outputs will be different, which will make Witness Generation crash.
        // This has been discussed between @evl and @deniallugo as a terrible solution, but best so far.
        let validation_computational_gas_limit =
            state_keeper_config.validation_computational_gas_limit;
        // This variable is fine to be loaded from config.
        // Today, this config changes only during regenesis.
        // In the hyper-chains world, this code will be refactored (using the DB saved value instead of config)
        let l2_chain_id = network_config.zksync_network_id;

        Ok(BasicWitnessInputProducer {
            connection_pool,
            validation_computational_gas_limit,
            object_store: store_factory.create_store().await.into(),
            l2_chain_id,
        })
    }

    fn process_job_impl(
        rt_handle: Handle,
        job: BasicWitnessInputProducerJob,
        started_at: Instant,
        connection_pool: ConnectionPool,
        validation_computational_gas_limit: u32,
        l2_chain_id: L2ChainId,
    ) -> anyhow::Result<WitnessBlockState> {
        let connection = rt_handle.block_on(connection_pool.access_storage())?;

        let (mut vm, storage_view) = create_vm(
            rt_handle.clone(),
            job.l1_batch_number,
            connection,
            validation_computational_gas_limit,
            l2_chain_id,
        );

        let mut connection = rt_handle
            .block_on(connection_pool.access_storage())
            .unwrap();
        let miniblock_and_transactions = rt_handle.block_on(
            connection
                .transactions_dal()
                .get_miniblock_with_transactions_for_l1_batch(job.l1_batch_number),
        );
        tracing::info!("Started execution of l1_batch: {:?}", job.l1_batch_number);
        for (miniblock, txs) in miniblock_and_transactions {
            tracing::debug!("Started execution of miniblock: {miniblock:?}");
            for tx in txs {
                tracing::debug!("Started execution of tx: {tx:?}");
                execute_tx(&tx, &mut vm);
                tracing::debug!("Finished execution of tx: {tx:?}");
            }
            let miniblock_state =
                rt_handle.block_on(get_miniblock_transition_state(&mut connection, miniblock));
            vm.start_new_l2_block(miniblock_state);
            tracing::debug!("Finished execution of miniblock: {miniblock:?}");
        }
        vm.finish_batch();
        tracing::info!("Finished execution of l1_batch: {:?}", job.l1_batch_number);

        metrics::histogram!(
            "basic_witness_input_producer.input_producer_time",
            started_at.elapsed(),
        );
        tracing::info!(
            "BasicWitnessInputProducer took {:?} for L1BatchNumber {}",
            started_at.elapsed(),
            job.l1_batch_number.0
        );

        let witness_block_state = (*storage_view).borrow().witness_block_state();
        Ok(witness_block_state)
    }
}

#[async_trait]
impl JobProcessor for BasicWitnessInputProducer {
    type Job = BasicWitnessInputProducerJob;
    type JobId = L1BatchNumber;
    type JobArtifacts = WitnessBlockState;
    const SERVICE_NAME: &'static str = "";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut connection = self
            .connection_pool
            .access_storage()
            .await
            .expect("couldn't get a connection from the pool");
        let l1_batch_to_process = connection
            .basic_witness_input_producer_dal()
            .get_next_basic_witness_input_producer_job()
            .await;
        match l1_batch_to_process {
            Some(number) => Ok(Some((
                number,
                BasicWitnessInputProducerJob {
                    l1_batch_number: number,
                },
            ))),
            None => Ok(None),
        }
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
            .expect("didn't receive number of attempts from database");
        tracing::warn!(
            "Failed to process job: {:?}, attempts: {}",
            job_id,
            attempts
        );
    }

    async fn process_job(
        &self,
        job: Self::Job,
        started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        let validation_computational_gas_limit = self.validation_computational_gas_limit;
        let l2_chain_id = self.l2_chain_id;
        let connection_pool = self.connection_pool.clone();
        let result = tokio::task::spawn_blocking(move || {
            let rt_handle = tokio::runtime::Handle::current();
            Self::process_job_impl(
                rt_handle,
                job,
                started_at,
                connection_pool.clone(),
                validation_computational_gas_limit,
                l2_chain_id,
            )
        });
        result
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
    ) -> anyhow::Result<()> {
        let upload_started_at = Instant::now();
        let _object_path = self.object_store.put(job_id, &artifacts).await?;
        metrics::histogram!(
            "basic_witness_input_producer.upload_input_time",
            upload_started_at.elapsed(),
        );
        let mut connection = self.connection_pool.access_storage().await?;
        connection
            .basic_witness_input_producer_dal()
            .mark_job_as_successful(job_id, started_at)
            .await;
        Ok(())
    }
}
