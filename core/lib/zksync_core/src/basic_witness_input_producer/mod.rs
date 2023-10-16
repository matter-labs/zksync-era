use async_trait::async_trait;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;
use tokio::task::JoinHandle;

use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    AccountTreeId, Address, L1BatchNumber, L2ChainId, MiniblockNumber, StorageKey, Transaction,
    H256,
};

use crate::state_keeper::io::common::{load_l1_batch_params, load_pending_batch};
use crate::state_keeper::io::PendingBatchData;
use crate::state_keeper::{L1BatchExecutorBuilder, MainBatchExecutorBuilder};
use crate::sync_layer::sync_action::SyncAction::Miniblock;
use crate::Component::StateKeeper;
use anyhow::Context;
use multivm::VmInstance;
use std::thread;
use std::time::{Duration, Instant};
use vm::{HistoryEnabled, L2BlockEnv, Vm};
use zksync_config::configs::chain::{NetworkConfig, StateKeeperConfig};
use zksync_config::constants::{
    SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
};
use zksync_config::{ContractsConfig, DBConfig};
use zksync_dal::basic_witness_input_producer_dal::BasicWitnessInputProducerStatus;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_state::{PostgresStorage, PostgresStorageCaches, ReadStorage, StorageView};
use zksync_types::block::unpack_block_info;
use zksync_types::witness_block_state::WitnessBlockState;
use zksync_utils::h256_to_u256;

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

async fn get_miniblock_transition_state(
    connection: &mut StorageProcessor<'_>,
    miniblock_number: MiniblockNumber,
) -> L2BlockEnv {
    let l2_block_info_key = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    );
    let l2_block_info = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(&l2_block_info_key, miniblock_number + 1)
        .await
        .unwrap();

    let (next_miniblock_number, next_miniblock_timestamp) =
        unpack_block_info(h256_to_u256(l2_block_info));

    let miniblock_hash = connection
        .blocks_web3_dal()
        .get_miniblock_hash(miniblock_number)
        .await
        .unwrap()
        .unwrap();

    let virtual_blocks = connection
        .blocks_dal()
        .get_virtual_blocks_for_miniblock(&(miniblock_number + 1))
        .await
        .unwrap()
        .unwrap();

    L2BlockEnv {
        number: next_miniblock_number as u32,
        timestamp: next_miniblock_timestamp,
        prev_block_hash: miniblock_hash,
        max_virtual_blocks_to_create: virtual_blocks,
    }
}

fn execute_tx<S: ReadStorage>(tx: &Transaction, vm: &mut VmInstance<S, HistoryEnabled>) {
    vm.make_snapshot();
    if let Ok(_) = vm.inspect_transaction_with_bytecode_compression(vec![], tx.clone(), true) {
        vm.pop_snapshot_no_rollback();
        return;
    }
    vm.rollback_to_the_latest_snapshot();
    vm.inspect_transaction_with_bytecode_compression(vec![], tx.clone(), false)
        .expect("Compression can't fail if we don't apply it");
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

    async fn process_job_impl(
        job: BasicWitnessInputProducerJob,
        started_at: Instant,
        connection_pool: ConnectionPool,
        validation_computational_gas_limit: u32,
        l2_chain_id: L2ChainId,
    ) -> anyhow::Result<WitnessBlockState> {
        let l1_batch_number = job.l1_batch_number;
        let prev_l1_batch_number = L1BatchNumber(l1_batch_number.0 - 1);

        let mut connection = connection_pool.access_storage().await?;
        let miniblock_number = connection
            .blocks_dal()
            .get_last_miniblock_for_l1_batch(&prev_l1_batch_number)
            .await
            .context(format!(
                "get_last_miniblock_for_l1_batch({prev_l1_batch_number:?})"
            ))?
            .expect(&format!(
                "l1_batch_number {:?} must have a previous miniblock to start from",
                l1_batch_number
            ));

        let fee_account_addr = connection
            .blocks_dal()
            .get_fee_address_for_l1_batch(&l1_batch_number)
            .await?
            .expect(&format!(
                "l1_batch_number {:?} must have fee_address_account",
                l1_batch_number
            ));
        let (system_env, l1_batch_env) = load_l1_batch_params(
            &mut connection,
            l1_batch_number,
            fee_account_addr,
            validation_computational_gas_limit,
            l2_chain_id,
        )
        .await
        .unwrap();

        drop(connection);

        let connection_pool = connection_pool.clone();
        // This spawn_blocking is needed given PostgresStorage's interface is a mix of blocking and non-blocking
        // The interface may be either refactored or broken down in a async and sync interface.
        tokio::task::spawn_blocking(move || {
            let rt_handle = tokio::runtime::Handle::current();
            let connection = rt_handle
                .block_on(connection_pool.access_storage())
                .unwrap();
            let pg_storage =
                PostgresStorage::new(rt_handle.clone(), connection, miniblock_number, true)
                    .with_caches(PostgresStorageCaches::new(
                        128 * 1_024 * 1_024, // 128MB -- picked as the default value used in EN
                        128 * 1_024 * 1_024, // 128MB -- picked as the default value used in EN
                    ));
            let storage_view = StorageView::new(pg_storage).to_rc_ptr();

            let mut vm = VmInstance::new(l1_batch_env, system_env, storage_view.clone());

            let mut connection = rt_handle
                .block_on(connection_pool.access_storage())
                .unwrap();
            let miniblock_and_transactions = rt_handle.block_on(
                connection
                    .transactions_dal()
                    .get_miniblock_with_transactions_for_l1_batch(l1_batch_number),
            );
            tracing::info!("Started execution of l1_batch: {l1_batch_number:?}");
            for (miniblock, txs) in miniblock_and_transactions {
                tracing::debug!("Started execution of miniblock: {miniblock:?}");
                for tx in txs {
                    execute_tx(&tx, &mut vm);
                }
                let miniblock_state =
                    rt_handle.block_on(get_miniblock_transition_state(&mut connection, miniblock));
                vm.start_new_l2_block(miniblock_state);
                tracing::debug!("Finished execution of miniblock: {miniblock:?}");
            }
            vm.finish_batch();
            tracing::info!("Finished execution of l1_batch: {l1_batch_number:?}");

            metrics::histogram!(
                "basic_witness_input_producer.input_producer_time",
                started_at.elapsed(),
            );
            tracing::info!(
                "BasicWitnessInputProducer took {:?} for L1BatchNumber {}",
                started_at.elapsed(),
                l1_batch_number.0
            );

            let witness_block_state = (*storage_view).borrow().witness_block_state();
            Ok(witness_block_state)
        })
        .await?
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
        self.connection_pool
            .access_storage()
            .await
            .unwrap()
            .basic_witness_input_producer_dal()
            .mark_job_as_failed(job_id, started_at, error)
            .await;
        // TODO: How about some nice logs on number of attempts here? We can load it form DB
    }

    async fn process_job(
        &self,
        job: Self::Job,
        started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        tokio::spawn(Self::process_job_impl(
            job,
            started_at,
            self.connection_pool.clone(),
            self.validation_computational_gas_limit,
            self.l2_chain_id,
        ))
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
    ) -> anyhow::Result<()> {
        let upload_started_at = Instant::now();
        let object_path = self.object_store.put(job_id, &artifacts).await?;
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
