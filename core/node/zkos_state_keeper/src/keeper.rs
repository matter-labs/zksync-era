use std::sync::Arc;
use std::time::Duration;
use anyhow::Context;
use ruint::aliases::U256;
use ruint::aliases::B160;
use zk_os_forward_system::run::{BatchContext, BatchOutput, PreimageSource, run_batch, StorageCommitment};
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::info_span;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_state::ReadStorageFactory;
use zksync_state_keeper::io::IoCursor;
use zksync_state_keeper::MempoolGuard;
use zksync_types::Transaction;
use crate::preimage_source::ZkSyncPreimageSource;
use crate::single_tx_source::SingleTxSource;
use crate::tree::InMemoryTree;

const POLL_WAIT_DURATION: Duration = Duration::from_millis(50);

pub struct ZkOsStateKeeper {
    stop_receiver: watch::Receiver<bool>,
    tree: InMemoryTree,
    pool: ConnectionPool<Core>,
    // storage_factory: Arc<dyn ReadStorageFactory>,
    mempool: MempoolGuard,
}

impl ZkOsStateKeeper {
    pub fn new(
        stop_receiver: watch::Receiver<bool>,
        pool: ConnectionPool<Core>,
        // storage_factory: Arc<dyn ReadStorageFactory>,
        mempool: MempoolGuard,
    ) -> Self {
        Self {
            stop_receiver,
            tree: InMemoryTree::new(),
            pool,
            // storage_factory,
            mempool,
        }
    }
    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!("Starting ZkOs StateKeeper");

        let mut connection = self.pool.connection_tagged("state_keeper").await?;

        // todo: IoCursor supports snapshot recovery, but it's not supported by zkos integration
        let mut cursor = IoCursor::new(&mut connection).await?;
        tracing::info!("Starting with cursor {:?}", cursor);


        while !self.is_canceled() {
            let storage_commitment = StorageCommitment {
                root: Default::default(),
                next_free_slot: 0,
            };
            //
            // let storage = self.storage_factory.access_storage(
            //     &self.stop_receiver,
            //     cursor.l1_batch,
            // ).await
            //     .context("Failed to access storage")?
            //     .context("Storage access was interrupted")?;

            let preimage_source = ZkSyncPreimageSource::new(
                self
                    .pool
                    .access_storage(&self.stop_receiver, cursor.l1_batch - 1)
                    .await?
                    .context("Failed to access storage")?
            );
            // todo: wait for the first transaction in mempool to prevent block being open for a while

            tracing::info!("Waiting for the next transaction");

            let Some(tx) = self
                .wait_for_next_tx()
                .await else {
                return Ok(())
            };
            tracing::info!("Transaction found: {:?}", tx);

            let tx_source = SingleTxSource::new(tx);

            let context = BatchContext {
                eip1559_basefee: U256::from(1),
                ergs_price: U256::from(1),
                // todo: consider changing type
                block_number: cursor.next_l2_block.0 as u64,
                timestamp: 0,
            };

            tracing::info!("Running batch");
            let result = run_batch(
                context,
                storage_commitment,
                self.tree.clone(),
                preimage_source,
                tx_source,
            );

            match result {
                Ok(result) => {
                    tracing::info!("Batch executed successfully: ${:?}", result);
                }
                Err(_) => {
                    tracing::error!("Error running batch");
                }
            }
        }
        Ok(())
    }
    async fn wait_for_next_tx(&mut self) -> Option<Transaction> {

        // todo: use proper filter
        let filter = L2TxFilter {
            fee_input: Default::default(),
            fee_per_gas: 0,
            gas_per_pubdata: 0,
        };

        let started_at = Instant::now();
        // or is it better to limit this?
        while !self.is_canceled() {
            // let get_latency = KEEPER_METRICS.get_tx_from_mempool.start();
            let maybe_tx = self.mempool.next_transaction(&filter);
            // get_latency.observe();

            if let Some(tx) = maybe_tx {
                // Reject transactions with too big gas limit. They are also rejected on the API level, but
                // we need to secure ourselves in case some tx will somehow get into mempool.
                // if tx.gas_limit() > self.max_allowed_tx_gas_limit {
                //     tracing::warn!(
                //     "Found tx with too big gas limit in state keeper, hash: {:?}, gas_limit: {}",
                //     tx.hash(),
                //     tx.gas_limit()
                // );
                //     self.reject(&tx, UnexecutableReason::Halt(Halt::TooBigGasLimit))
                //         .await?;
                //     continue;
                // }
                return Some(tx);
            }
            tokio::time::sleep(POLL_WAIT_DURATION).await;
        }
        None
    }

    fn is_canceled(&self) -> bool {
        *self.stop_receiver.borrow()
    }
}