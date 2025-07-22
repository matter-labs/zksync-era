use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Context as _;
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::watch;
use zksync_config::configs::chain::MempoolConfig;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_multivm::utils::derive_base_fee_and_gas_per_pubdata;
use zksync_node_fee_model::BatchFeeModelInputProvider;
#[cfg(test)]
use zksync_types::H256;
use zksync_types::{get_nonce_key, Address, Nonce, ProtocolVersionId};

use super::{mempool_guard::MempoolGuard, metrics::KEEPER_METRICS};

/// Creates a mempool filter for L2 transactions based on the current L1 gas price.
/// The filter is used to filter out transactions from the mempool that do not cover expenses
/// to process them.
pub async fn l2_tx_filter(
    batch_fee_input_provider: &dyn BatchFeeModelInputProvider,
    protocol_version: ProtocolVersionId,
) -> anyhow::Result<L2TxFilter> {
    let fee_input = batch_fee_input_provider.get_batch_fee_input().await?;
    let (base_fee, gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(fee_input, protocol_version.into());
    Ok(L2TxFilter {
        fee_input,
        fee_per_gas: base_fee,
        gas_per_pubdata: gas_per_pubdata as u32,
        protocol_version,
    })
}

#[derive(Debug)]
pub struct MempoolFetcher {
    mempool: MempoolGuard,
    pool: ConnectionPool<Core>,
    batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    sync_interval: Duration,
    sync_batch_size: usize,
    stuck_tx_timeout: Option<Duration>,
    l1_to_l2_txs_paused: bool,
    #[cfg(test)]
    transaction_hashes_sender: mpsc::UnboundedSender<Vec<H256>>,
}
impl MempoolFetcher {
    pub fn new(
        mempool: MempoolGuard,
        batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
        config: &MempoolConfig,
        pool: ConnectionPool<Core>,
    ) -> Self {
        Self {
            mempool,
            pool,
            batch_fee_input_provider,
            sync_interval: config.sync_interval,
            sync_batch_size: config.sync_batch_size,
            stuck_tx_timeout: config.remove_stuck_txs.then_some(config.stuck_tx_timeout),
            l1_to_l2_txs_paused: config.l1_to_l2_txs_paused,
            #[cfg(test)]
            transaction_hashes_sender: mpsc::unbounded_channel().0,
        }
    }

    pub async fn run(mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut storage = self.pool.connection_tagged("state_keeper").await?;
        if let Some(stuck_tx_timeout) = self.stuck_tx_timeout {
            let removed_txs = storage
                .transactions_dal()
                .remove_stuck_txs(stuck_tx_timeout)
                .await
                .context("failed removing stuck transactions")?;
            tracing::info!("Number of stuck txs was removed: {removed_txs}");
        }
        storage.transactions_dal().reset_mempool().await?;
        drop(storage);

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, mempool is shutting down");
                break;
            }
            let latency = KEEPER_METRICS.mempool_sync.start();
            let mut connection = self.pool.connection_tagged("state_keeper").await?;
            let mut storage_transaction = connection.start_transaction().await?;
            let mempool_info = self.mempool.get_mempool_info();

            KEEPER_METRICS
                .mempool_stashed_accounts
                .set(mempool_info.stashed_accounts.len());
            KEEPER_METRICS
                .mempool_purged_accounts
                .set(mempool_info.purged_accounts.len());

            let protocol_version = storage_transaction
                .blocks_dal()
                .pending_protocol_version()
                .await
                .context("failed getting pending protocol version")?;

            let (fee_per_gas, gas_per_pubdata) = if let Some(unsealed_batch) = storage_transaction
                .blocks_dal()
                .get_unsealed_l1_batch()
                .await
                .context("failed getting unsealed batch")?
            {
                let (fee_per_gas, gas_per_pubdata) = derive_base_fee_and_gas_per_pubdata(
                    unsealed_batch.fee_input,
                    protocol_version.into(),
                );
                (fee_per_gas, gas_per_pubdata as u32)
            } else {
                let filter = l2_tx_filter(self.batch_fee_input_provider.as_ref(), protocol_version)
                    .await
                    .context("failed creating L2 transaction filter")?;

                (filter.fee_per_gas, filter.gas_per_pubdata)
            };

            let transactions_with_constraints = storage_transaction
                .transactions_dal()
                .sync_mempool(
                    &mempool_info.stashed_accounts,
                    &mempool_info.purged_accounts,
                    gas_per_pubdata,
                    fee_per_gas,
                    !self.l1_to_l2_txs_paused,
                    self.sync_batch_size,
                )
                .await
                .context("failed syncing mempool")?;
            storage_transaction.commit().await?;

            #[cfg(test)]
            let transaction_hashes: Vec<_> = transactions_with_constraints
                .iter()
                .map(|(t, _c)| t.hash())
                .collect();
            let all_transactions_loaded =
                transactions_with_constraints.len() < self.sync_batch_size;

            // We should be careful about what nonces we provide for mempool.
            // There are 2 actions that must be done sequentially so that the nonces in mempool are always correct:
            // a) "load nonces from postgres and insert to mempool", it's done in the loop below
            // b) "update nonces in mempool after processed block", it's done in state keeper right after block is fully sealed.
            // This is why `self.mempool.enter_critical()` is called.
            // We also insert txs in small chunks, so that it doesn't block state keeper code for too long.
            const CHUNK_SIZE: usize = 100;
            for chunk in transactions_with_constraints.chunks(CHUNK_SIZE) {
                let chunk = chunk.to_vec();
                let addresses: Vec<_> =
                    chunk.iter().map(|(tx, _)| tx.initiator_account()).collect();

                let _guard = self.mempool.enter_critical();
                let nonces = get_nonces(&mut connection, &addresses).await?;
                self.mempool.insert(chunk, nonces);
            }
            drop(connection);

            #[cfg(test)]
            self.transaction_hashes_sender.send(transaction_hashes).ok();
            latency.observe();

            if all_transactions_loaded {
                tokio::time::sleep(self.sync_interval).await;
            }
        }
        Ok(())
    }
}

/// Loads nonces for all addresses from the storage.
async fn get_nonces(
    storage: &mut Connection<'_, Core>,
    addresses: &[Address],
) -> anyhow::Result<HashMap<Address, Nonce>> {
    let (nonce_keys, address_by_nonce_key): (Vec<_>, HashMap<_, _>) = addresses
        .iter()
        .map(|address| {
            let nonce_key = get_nonce_key(address).hashed_key();
            (nonce_key, (nonce_key, *address))
        })
        .unzip();

    let nonce_values = storage
        .storage_web3_dal()
        .get_values(&nonce_keys)
        .await
        .context("failed getting nonces from storage")?;

    Ok(nonce_values
        .into_iter()
        .map(|(nonce_key, nonce_value)| {
            // `unwrap()` is safe by construction.
            let be_u32_bytes: [u8; 4] = nonce_value[28..].try_into().unwrap();
            let nonce = Nonce(u32::from_be_bytes(be_u32_bytes));
            (address_by_nonce_key[&nonce_key], nonce)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use zksync_multivm::interface::{tracer::ValidationTraces, TransactionExecutionMetrics};
    use zksync_node_fee_model::MockBatchFeeParamsProvider;
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_node_test_utils::create_l2_transaction;
    use zksync_types::{
        u256_to_h256, L2BlockNumber, PriorityOpId, ProtocolVersionId, StorageLog, H256,
    };

    use super::*;

    const TEST_MEMPOOL_CONFIG: MempoolConfig = MempoolConfig {
        sync_interval: Duration::from_millis(10),
        sync_batch_size: 100,
        capacity: 100,
        stuck_tx_timeout: Duration::ZERO,
        remove_stuck_txs: false,
        delay_interval: Duration::from_millis(10),
        l1_to_l2_txs_paused: false,
        high_priority_l2_tx_initiator: None,
        high_priority_l2_tx_protocol_version: Some(29),
    };

    #[tokio::test]
    async fn getting_transaction_nonces() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();

        let transaction = create_l2_transaction(10, 100);
        let transaction_initiator = transaction.initiator_account();
        let nonce_key = get_nonce_key(&transaction_initiator);
        let nonce_log = StorageLog::new_write_log(nonce_key, u256_to_h256(42.into()));
        storage
            .storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(0), &[nonce_log])
            .await
            .unwrap();

        let other_transaction = create_l2_transaction(10, 100);
        let other_transaction_initiator = other_transaction.initiator_account();
        assert_ne!(other_transaction_initiator, transaction_initiator);

        let nonces = get_nonces(
            &mut storage,
            &[transaction_initiator, other_transaction_initiator],
        )
        .await
        .unwrap();
        assert_eq!(
            nonces,
            HashMap::from([
                (transaction_initiator, Nonce(42)),
                (other_transaction_initiator, Nonce(0)),
            ])
        );
    }

    #[tokio::test]
    async fn syncing_mempool_basics() {
        let pool = ConnectionPool::constrained_test_pool(1).await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        drop(storage);

        let mempool = MempoolGuard::new(
            PriorityOpId(0),
            100,
            TEST_MEMPOOL_CONFIG.high_priority_l2_tx_initiator,
            TEST_MEMPOOL_CONFIG
                .high_priority_l2_tx_protocol_version
                .map(|v| (v as u16).try_into().unwrap()),
        );
        let fee_params_provider: Arc<dyn BatchFeeModelInputProvider> =
            Arc::new(MockBatchFeeParamsProvider::default());
        let fee_input = fee_params_provider.get_batch_fee_input().await.unwrap();
        let (base_fee, gas_per_pubdata) =
            derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());

        let mut fetcher = MempoolFetcher::new(
            mempool.clone(),
            fee_params_provider,
            &TEST_MEMPOOL_CONFIG,
            pool.clone(),
        );
        let (tx_hashes_sender, mut tx_hashes_receiver) = mpsc::unbounded_channel();
        fetcher.transaction_hashes_sender = tx_hashes_sender;
        let (stop_sender, stop_receiver) = watch::channel(false);
        let fetcher_task = tokio::spawn(fetcher.run(stop_receiver));

        // Add a new transaction to the storage.
        let transaction = create_l2_transaction(base_fee, gas_per_pubdata);
        let transaction_hash = transaction.hash();
        let mut storage = pool.connection().await.unwrap();
        storage
            .transactions_dal()
            .insert_transaction_l2(
                &transaction,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        drop(storage);

        // Check that the transaction is eventually synced.
        let tx_hashes = wait_for_new_transactions(&mut tx_hashes_receiver).await;
        assert_eq!(tx_hashes, [transaction_hash]);
        assert_eq!(mempool.stats().l2_transaction_count, 1);

        stop_sender.send_replace(true);
        fetcher_task.await.unwrap().expect("fetcher errored");
    }

    async fn wait_for_new_transactions(
        tx_hashes_receiver: &mut mpsc::UnboundedReceiver<Vec<H256>>,
    ) -> Vec<H256> {
        loop {
            let tx_hashes = tx_hashes_receiver.recv().await.unwrap();
            if tx_hashes.is_empty() {
                continue;
            }
            break tx_hashes;
        }
    }

    #[tokio::test]
    async fn ignoring_transaction_with_insufficient_fee() {
        let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        drop(storage);

        let mempool = MempoolGuard::new(
            PriorityOpId(0),
            100,
            TEST_MEMPOOL_CONFIG.high_priority_l2_tx_initiator,
            TEST_MEMPOOL_CONFIG
                .high_priority_l2_tx_protocol_version
                .map(|v| (v as u16).try_into().unwrap()),
        );
        let fee_params_provider: Arc<dyn BatchFeeModelInputProvider> =
            Arc::new(MockBatchFeeParamsProvider::default());
        let fee_input = fee_params_provider.get_batch_fee_input().await.unwrap();
        let (base_fee, gas_per_pubdata) =
            derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());

        let fetcher = MempoolFetcher::new(
            mempool.clone(),
            fee_params_provider,
            &TEST_MEMPOOL_CONFIG,
            pool.clone(),
        );
        let (stop_sender, stop_receiver) = watch::channel(false);
        let fetcher_task = tokio::spawn(fetcher.run(stop_receiver));

        // Add a transaction with insufficient fee to the storage.
        let transaction = create_l2_transaction(base_fee / 2, gas_per_pubdata / 2);
        let mut storage = pool.connection().await.unwrap();
        storage
            .transactions_dal()
            .insert_transaction_l2(
                &transaction,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        drop(storage);

        tokio::time::sleep(TEST_MEMPOOL_CONFIG.sync_interval * 5).await;
        assert_eq!(mempool.stats().l2_transaction_count, 0);

        stop_sender.send_replace(true);
        fetcher_task.await.unwrap().expect("fetcher errored");
    }

    #[tokio::test]
    async fn ignoring_transaction_with_old_nonce() {
        let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        drop(storage);

        let mempool = MempoolGuard::new(
            PriorityOpId(0),
            100,
            TEST_MEMPOOL_CONFIG.high_priority_l2_tx_initiator,
            TEST_MEMPOOL_CONFIG
                .high_priority_l2_tx_protocol_version
                .map(|v| (v as u16).try_into().unwrap()),
        );
        let fee_params_provider: Arc<dyn BatchFeeModelInputProvider> =
            Arc::new(MockBatchFeeParamsProvider::default());
        let fee_input = fee_params_provider.get_batch_fee_input().await.unwrap();
        let (base_fee, gas_per_pubdata) =
            derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());

        let mut fetcher = MempoolFetcher::new(
            mempool.clone(),
            fee_params_provider,
            &TEST_MEMPOOL_CONFIG,
            pool.clone(),
        );
        let (tx_hashes_sender, mut tx_hashes_receiver) = mpsc::unbounded_channel();
        fetcher.transaction_hashes_sender = tx_hashes_sender;
        let (stop_sender, stop_receiver) = watch::channel(false);
        let fetcher_task = tokio::spawn(fetcher.run(stop_receiver));

        // Add a new transaction to the storage.
        let transaction = create_l2_transaction(base_fee * 2, gas_per_pubdata * 2);
        assert_eq!(transaction.nonce(), Nonce(0));
        let transaction_hash = transaction.hash();
        let nonce_key = get_nonce_key(&transaction.initiator_account());
        let nonce_log = StorageLog::new_write_log(nonce_key, u256_to_h256(42.into()));
        let mut storage = pool.connection().await.unwrap();
        storage
            .storage_logs_dal()
            .append_storage_logs(L2BlockNumber(0), &[nonce_log])
            .await
            .unwrap();
        storage
            .transactions_dal()
            .insert_transaction_l2(
                &transaction,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        drop(storage);

        // Check that the transaction is eventually synced.
        let tx_hashes = wait_for_new_transactions(&mut tx_hashes_receiver).await;
        assert_eq!(tx_hashes, [transaction_hash]);
        // Transaction must not be added to the pool because of its outdated nonce.
        assert_eq!(mempool.stats().l2_transaction_count, 0);

        stop_sender.send_replace(true);
        fetcher_task.await.unwrap().expect("fetcher errored");
    }
}
