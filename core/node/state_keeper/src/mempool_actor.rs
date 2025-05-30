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
use zksync_types::{get_nonce_key, vm::VmVersion, Address, Nonce, Transaction};

use super::{metrics::KEEPER_METRICS, types::MempoolGuard};
use crate::v26_utils::find_unsafe_deposit;

/// Creates a mempool filter for L2 transactions based on the current L1 gas price.
/// The filter is used to filter out transactions from the mempool that do not cover expenses
/// to process them.
pub async fn l2_tx_filter(
    batch_fee_input_provider: &dyn BatchFeeModelInputProvider,
    vm_version: VmVersion,
) -> anyhow::Result<L2TxFilter> {
    let fee_input = batch_fee_input_provider.get_batch_fee_input().await?;
    let (base_fee, gas_per_pubdata) = derive_base_fee_and_gas_per_pubdata(fee_input, vm_version);
    Ok(L2TxFilter {
        fee_input,
        fee_per_gas: base_fee,
        gas_per_pubdata: gas_per_pubdata as u32,
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
    skip_unsafe_deposit_checks: bool,
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
            skip_unsafe_deposit_checks: config.skip_unsafe_deposit_checks,
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
                let filter = l2_tx_filter(
                    self.batch_fee_input_provider.as_ref(),
                    protocol_version.into(),
                )
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

            let unsafe_deposit = if !self.skip_unsafe_deposit_checks {
                find_unsafe_deposit(
                    transactions_with_constraints.iter().map(|(tx, _)| tx),
                    &mut storage_transaction,
                )
                .await?
            } else {
                // We do not check for the unsafe deposits, so we just treat all deposits as "safe"
                None
            };

            let transactions_with_constraints = if let Some(hash) = unsafe_deposit {
                tracing::warn!("Transaction with hash {:#?} is an unsafe deposit. All L1->L2 transactions are returned to mempool.", hash);

                let hashes: Vec<_> = transactions_with_constraints
                    .iter()
                    .map(|x| x.0.hash())
                    .collect();

                storage_transaction
                    .transactions_dal()
                    .reset_mempool_status(&hashes)
                    .await
                    .context("failed to return txs to mempool")?;

                storage_transaction
                    .transactions_dal()
                    .sync_mempool(
                        &mempool_info.stashed_accounts,
                        &mempool_info.purged_accounts,
                        gas_per_pubdata,
                        fee_per_gas,
                        false,
                        self.sync_batch_size,
                    )
                    .await
                    .context("failed syncing mempool")?
            } else {
                transactions_with_constraints
            };

            let transactions: Vec<_> = transactions_with_constraints
                .iter()
                .map(|(t, _c)| t)
                .collect();

            let nonces = get_transaction_nonces(&mut storage_transaction, &transactions).await?;

            storage_transaction.commit().await?;
            drop(connection);

            #[cfg(test)]
            {
                let transaction_hashes = transactions.iter().map(|x| x.hash()).collect();
                self.transaction_hashes_sender.send(transaction_hashes).ok();
            }
            let all_transactions_loaded = transactions.len() < self.sync_batch_size;
            self.mempool.insert(transactions_with_constraints, nonces);
            latency.observe();

            if all_transactions_loaded {
                tokio::time::sleep(self.sync_interval).await;
            }
        }
        Ok(())
    }
}

/// Loads nonces for all distinct `transactions` initiators from the storage.
async fn get_transaction_nonces(
    storage: &mut Connection<'_, Core>,
    transactions: &[&Transaction],
) -> anyhow::Result<HashMap<Address, Nonce>> {
    let (nonce_keys, address_by_nonce_key): (Vec<_>, HashMap<_, _>) = transactions
        .iter()
        .map(|tx| {
            let address = tx.initiator_account();
            let nonce_key = get_nonce_key(&address).hashed_key();
            (nonce_key, (nonce_key, address))
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
        skip_unsafe_deposit_checks: false,
        l1_to_l2_txs_paused: false,
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

        let nonces = get_transaction_nonces(
            &mut storage,
            &[&transaction.into(), &other_transaction.into()],
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

        let mempool = MempoolGuard::new(PriorityOpId(0), 100);
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

        let mempool = MempoolGuard::new(PriorityOpId(0), 100);
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

        let mempool = MempoolGuard::new(PriorityOpId(0), 100);
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
