use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use chrono::{TimeZone, Utc};
use tokio::sync::{watch, RwLock};
use zksync_dal::{
    helpers::wait_for_l1_batch, transactions_dal::L2TxSubmissionResult, Connection, ConnectionPool,
    Core, CoreDal, DalError,
};
use zksync_multivm::interface::tracer::ValidationTraces;
use zksync_shared_metrics::{TxStage, APP_METRICS};
use zksync_types::{api, l2::L2Tx, try_stoppable, Address, Nonce, StopContext, H256, U256};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::{ClientRpcContext, EnrichedClientResult, Web3Error},
    namespaces::EthNamespaceClient,
};

use super::{tx_sink::TxSink, SubmitTxError};
use crate::execution_sandbox::SandboxExecutionOutput;

/// In-memory transaction cache for a full node. Works like an ad-hoc mempool replacement, with the important limitation that
/// it's not synchronized across the network.
///
/// # Managing cache growth
///
/// To keep cache at reasonable size, the following garbage collection procedures are implemented:
///
/// - [`Self::run_updates()`] periodically gets nonces for all distinct accounts for the transactions in cache and removes
///   all transactions with stale nonces. This includes both transactions included into L2 blocks and replaced transactions.
/// - The same nonce filtering logic is applied for the transaction initiator address each time a transaction is fetched from cache.
///   We don't want to return such transactions if they are already included in an L2 block or replaced locally, but `Self::run_updates()`
///   hasn't run yet.
#[derive(Debug, Clone, Default)]
pub(crate) struct TxCache {
    inner: Arc<RwLock<TxCacheInner>>,
}

#[derive(Debug, Default)]
struct TxCacheInner {
    transactions_by_hash: HashMap<H256, L2Tx>,
    tx_hashes_by_initiator: HashMap<(Address, Nonce), HashSet<H256>>,
    nonces_by_account: HashMap<Address, BTreeSet<Nonce>>,
}

impl TxCacheInner {
    /// Removes transactions from the cache based on nonces for accounts loaded from Postgres.
    fn collect_garbage(&mut self, nonces_for_accounts: &HashMap<Address, Nonce>) {
        self.nonces_by_account.retain(|address, account_nonces| {
            let stored_nonce = nonces_for_accounts
                .get(address)
                .copied()
                .unwrap_or(Nonce(0));
            // Retain only nonces starting from the stored one, and remove transactions with all past nonces;
            // this includes both successfully executed and replaced transactions.
            let retained_nonces = account_nonces.split_off(&stored_nonce);
            for &nonce in &*account_nonces {
                if let Some(tx_hashes) = self.tx_hashes_by_initiator.remove(&(*address, nonce)) {
                    for tx_hash in tx_hashes {
                        self.transactions_by_hash.remove(&tx_hash);
                    }
                }
            }
            *account_nonces = retained_nonces;
            // If we've removed all nonces, drop the account entry so we don't request stored nonces for it later.
            !account_nonces.is_empty()
        });
    }

    /// Same as `collect_garbage()`, but optimized for a single `(account, nonce)` entry.
    fn collect_garbage_for_account(&mut self, initiator_address: Address, stored_nonce: Nonce) {
        let Some(account_nonces) = self.nonces_by_account.get_mut(&initiator_address) else {
            return;
        };

        let retained_nonces = account_nonces.split_off(&stored_nonce);
        for &nonce in &*account_nonces {
            if let Some(tx_hashes) = self
                .tx_hashes_by_initiator
                .remove(&(initiator_address, nonce))
            {
                for tx_hash in tx_hashes {
                    self.transactions_by_hash.remove(&tx_hash);
                }
            }
        }
        *account_nonces = retained_nonces;

        if account_nonces.is_empty() {
            self.nonces_by_account.remove(&initiator_address);
        }
    }
}

impl TxCache {
    async fn push(&self, tx: L2Tx) {
        let mut inner = self.inner.write().await;
        inner
            .nonces_by_account
            .entry(tx.initiator_account())
            .or_default()
            .insert(tx.nonce());
        inner
            .tx_hashes_by_initiator
            .entry((tx.initiator_account(), tx.nonce()))
            .or_default()
            .insert(tx.hash());
        inner.transactions_by_hash.insert(tx.hash(), tx);
    }

    async fn get(&self, tx_hash: H256) -> Option<L2Tx> {
        self.inner
            .read()
            .await
            .transactions_by_hash
            .get(&tx_hash)
            .cloned()
    }

    async fn remove(&self, tx_hash: H256) {
        let mut inner = self.inner.write().await;
        let Some(tx) = inner.transactions_by_hash.remove(&tx_hash) else {
            // The transaction is already removed; this is fine.
            return;
        };

        let initiator_and_nonce = (tx.initiator_account(), tx.nonce());
        if let Some(txs) = inner.tx_hashes_by_initiator.get_mut(&initiator_and_nonce) {
            txs.remove(&tx_hash);
            if txs.is_empty() {
                inner.tx_hashes_by_initiator.remove(&initiator_and_nonce);
                // No transactions with `initiator_and_nonce` remain in the cache; remove the nonce record as well
                if let Some(nonces) = inner.nonces_by_account.get_mut(&tx.initiator_account()) {
                    nonces.remove(&tx.nonce());
                    if nonces.is_empty() {
                        inner.nonces_by_account.remove(&tx.initiator_account());
                    }
                }
            }
        }
    }

    async fn get_nonces_for_account(&self, account_address: Address) -> BTreeSet<Nonce> {
        let inner = self.inner.read().await;
        if let Some(nonces) = inner.nonces_by_account.get(&account_address) {
            nonces.clone()
        } else {
            BTreeSet::new()
        }
    }

    async fn step(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let addresses: Vec<_> = {
            // Split into 2 statements for readability.
            let inner = self.inner.read().await;
            inner.nonces_by_account.keys().copied().collect()
        };
        if addresses.is_empty() {
            return Ok(()); // Do not spend time on acquiring a connection and executing a query
        }

        let mut storage = pool.connection_tagged("api").await?;
        let nonces_for_accounts = storage
            .storage_web3_dal()
            .get_nonces_for_addresses(&addresses)
            .await?;
        drop(storage); // Don't hold both `storage` and lock on `inner` at the same time.

        self.inner
            .write()
            .await
            .collect_garbage(&nonces_for_accounts);
        Ok(())
    }

    async fn run_updates(
        self,
        pool: ConnectionPool<Core>,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        const UPDATE_INTERVAL: Duration = Duration::from_secs(1);

        tracing::info!(
            "Waiting for at least one L1 batch in Postgres to start TxCache::run_updates"
        );
        // Starting the updater before L1 batches are present in Postgres can lead to some invariants the server logic
        // implicitly assumes not being upheld. The only case when we'll actually wait here is immediately after snapshot recovery.
        let earliest_l1_batch_number =
            try_stoppable!(
                wait_for_l1_batch(&pool, UPDATE_INTERVAL, &mut stop_receiver)
                    .await
                    .stop_context("error while waiting for L1 batch in Postgres")
            );
        tracing::info!("Successfully waited for at least one L1 batch in Postgres; the earliest one is #{earliest_l1_batch_number}");

        while !*stop_receiver.borrow() {
            self.step(&pool).await?;
            tokio::time::sleep(UPDATE_INTERVAL).await;
        }
        Ok(())
    }
}

/// Used by external node to proxy transaction to the main node
/// and store them while they're not synced back yet
#[derive(Debug)]
pub struct TxProxy {
    tx_cache: TxCache,
    client: Box<DynClient<L2>>,
}

impl TxProxy {
    pub fn new(client: Box<DynClient<L2>>) -> Self {
        Self {
            tx_cache: TxCache::default(),
            client: client.for_component("tx_proxy"),
        }
    }

    async fn submit_tx_impl(&self, tx: &L2Tx) -> EnrichedClientResult<H256> {
        let input_data = tx.common_data.input_data().expect("raw tx is absent");
        let raw_tx = zksync_types::web3::Bytes(input_data.to_vec());
        let tx_hash = tx.hash();
        tracing::info!("Proxying tx {tx_hash:?}");
        self.client
            .send_raw_transaction(raw_tx)
            .rpc_context("send_raw_transaction")
            .with_arg("tx_hash", &tx_hash)
            .await
    }

    async fn find_tx(
        &self,
        storage: &mut Connection<'_, Core>,
        tx_hash: H256,
    ) -> Result<Option<L2Tx>, Web3Error> {
        let Some(tx) = self.tx_cache.get(tx_hash).await else {
            return Ok(None);
        };

        let initiator_address = tx.initiator_account();
        let nonce_map = storage
            .storage_web3_dal()
            .get_nonces_for_addresses(&[initiator_address])
            .await
            .map_err(DalError::generalize)?;
        if let Some(&stored_nonce) = nonce_map.get(&initiator_address) {
            // `stored_nonce` is the *next* nonce of the `initiator_address` account, thus, strict inequality check
            if tx.nonce() < stored_nonce {
                // Transaction is included in a block or replaced; either way, it should be removed from the cache.
                self.tx_cache
                    .inner
                    .write()
                    .await
                    .collect_garbage_for_account(initiator_address, stored_nonce);
                return Ok(None);
            }
        }
        Ok(Some(tx))
    }

    async fn next_nonce_by_initiator_account(
        &self,
        account_address: Address,
        current_nonce: u32,
    ) -> Nonce {
        let mut pending_nonce = Nonce(current_nonce);
        let nonces = self.tx_cache.get_nonces_for_account(account_address).await;
        for nonce in nonces.range(pending_nonce + 1..) {
            // If nonce is not sequential, then we should not increment nonce.
            if nonce == &pending_nonce {
                pending_nonce += 1;
            } else {
                break;
            }
        }

        pending_nonce
    }

    pub fn account_nonce_sweeper_task(
        &self,
        pool: ConnectionPool<Core>,
    ) -> AccountNonceSweeperTask {
        let cache = self.tx_cache.clone();
        AccountNonceSweeperTask { cache, pool }
    }
}

#[derive(Debug)]
pub struct AccountNonceSweeperTask {
    cache: TxCache,
    pool: ConnectionPool<Core>,
}

impl AccountNonceSweeperTask {
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.cache.run_updates(self.pool, stop_receiver).await
    }
}

#[async_trait::async_trait]
impl TxSink for TxProxy {
    async fn submit_tx(
        &self,
        tx: &L2Tx,
        _execution_output: &SandboxExecutionOutput,
        _validation_traces: ValidationTraces,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        // We're running an external node: we have to proxy the transaction to the main node.
        // But before we do that, save the tx to cache in case someone will request it
        // Before it reaches the main node.
        self.tx_cache.push(tx.clone()).await;
        if let Err(err) = self.submit_tx_impl(tx).await {
            // Remove the transaction from the cache on failure so that it doesn't occupy space in the cache indefinitely.
            self.tx_cache.remove(tx.hash()).await;
            return Err(err.into());
        }
        APP_METRICS.processed_txs[&TxStage::Proxied].inc();
        Ok(L2TxSubmissionResult::Proxied)
    }

    async fn lookup_pending_nonce(
        &self,
        account_address: Address,
        last_known_nonce: u32,
    ) -> Result<Option<Nonce>, Web3Error> {
        // EN: get pending nonces from the transaction cache
        // We don't have mempool in EN, it's safe to use the proxy cache as a mempool
        Ok(Some(
            self.next_nonce_by_initiator_account(account_address, last_known_nonce)
                .await
                .0
                .into(),
        ))
    }

    async fn lookup_tx(
        &self,
        storage: &mut Connection<'_, Core>,
        id: api::TransactionId,
    ) -> Result<Option<api::Transaction>, Web3Error> {
        if let api::TransactionId::Hash(hash) = id {
            // If the transaction is not in the db, check the cache
            if let Some(tx) = self.find_tx(storage, hash).await? {
                // check nonce for initiator
                return Ok(Some(tx.into()));
            }
        }
        Ok(None)
    }

    async fn lookup_tx_details(
        &self,
        storage: &mut Connection<'_, Core>,
        hash: H256,
    ) -> Result<Option<api::TransactionDetails>, Web3Error> {
        if let Some(tx) = self.find_tx(storage, hash).await? {
            let received_at_ms =
                i64::try_from(tx.received_timestamp_ms).context("received timestamp overflow")?;
            let received_at = Utc
                .timestamp_millis_opt(received_at_ms)
                .single()
                .context("received timestamp overflow")?;
            return Ok(Some(api::TransactionDetails {
                is_l1_originated: false,
                status: api::TransactionStatus::Pending,
                fee: U256::zero(), // always zero for pending transactions
                gas_per_pubdata: tx.common_data.fee.gas_per_pubdata_limit,
                initiator_address: tx.initiator_account(),
                received_at,
                eth_commit_tx_hash: None,
                eth_prove_tx_hash: None,
                eth_execute_tx_hash: None,
                eth_precommit_tx_hash: None,
            }));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use test_casing::test_casing;
    use zksync_node_genesis::{insert_genesis_batch, mock_genesis_config, GenesisParams};
    use zksync_node_test_utils::{create_l2_block, create_l2_transaction};
    use zksync_types::{get_nonce_key, web3::Bytes, L2BlockNumber, StorageLog};
    use zksync_web3_decl::{client::MockClient, jsonrpsee::core::ClientError};

    use super::*;

    #[tokio::test]
    async fn tx_cache_basics() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        let params = GenesisParams::load_genesis_params(mock_genesis_config())
            .unwrap()
            .into();
        insert_genesis_batch(&mut storage, &params).await.unwrap();

        let tx = create_l2_transaction(10, 100);
        let send_tx_called = Arc::new(AtomicBool::new(false));
        let main_node_client = MockClient::builder(L2::default())
            .method("eth_sendRawTransaction", {
                let send_tx_called = send_tx_called.clone();
                let tx = tx.clone();
                move |bytes: Bytes| {
                    assert_eq!(bytes.0, tx.common_data.input_data().unwrap());
                    send_tx_called.store(true, Ordering::Relaxed);
                    Ok(tx.hash())
                }
            })
            .build();

        let proxy = TxProxy::new(Box::new(main_node_client));
        proxy
            .submit_tx(
                &tx,
                &SandboxExecutionOutput::mock_success(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        assert!(send_tx_called.load(Ordering::Relaxed));

        // Check that the transaction is present in the cache
        assert_eq!(proxy.tx_cache.get(tx.hash()).await.unwrap(), tx);
        let found_tx = proxy
            .lookup_tx(&mut storage, api::TransactionId::Hash(tx.hash()))
            .await
            .unwrap()
            .expect("no transaction");
        assert_eq!(found_tx.hash, tx.hash());

        let pending_nonce = proxy
            .lookup_pending_nonce(tx.initiator_account(), 0)
            .await
            .unwrap()
            .expect("no nonce");
        assert_eq!(pending_nonce, tx.nonce());

        let tx_details = proxy
            .lookup_tx_details(&mut storage, tx.hash())
            .await
            .unwrap()
            .expect("no transaction");
        assert_eq!(tx_details.initiator_address, tx.initiator_account());
    }

    #[tokio::test]
    async fn low_level_transaction_cache_operations() {
        let tx_cache = TxCache::default();
        let tx = create_l2_transaction(10, 100);
        let tx_hash = tx.hash();

        tx_cache.push(tx.clone()).await;
        assert_eq!(tx_cache.get(tx_hash).await.unwrap(), tx);
        assert_eq!(
            tx_cache
                .get_nonces_for_account(tx.initiator_account())
                .await,
            BTreeSet::from([Nonce(0)])
        );

        tx_cache.remove(tx_hash).await;
        assert_eq!(tx_cache.get(tx_hash).await, None);
        assert_eq!(
            tx_cache
                .get_nonces_for_account(tx.initiator_account())
                .await,
            BTreeSet::new()
        );

        {
            let inner = tx_cache.inner.read().await;
            assert!(inner.transactions_by_hash.is_empty(), "{inner:?}");
            assert!(inner.nonces_by_account.is_empty(), "{inner:?}");
            assert!(inner.tx_hashes_by_initiator.is_empty(), "{inner:?}");
        }
    }

    #[tokio::test]
    async fn low_level_transaction_cache_operations_with_replacing_transaction() {
        let tx_cache = TxCache::default();
        let tx = create_l2_transaction(10, 100);
        let tx_hash = tx.hash();
        let mut replacing_tx = create_l2_transaction(10, 100);
        replacing_tx.common_data.initiator_address = tx.initiator_account();
        let replacing_tx_hash = replacing_tx.hash();
        assert_ne!(replacing_tx_hash, tx_hash);

        tx_cache.push(tx.clone()).await;
        tx_cache.push(replacing_tx).await;
        tx_cache.get(tx_hash).await.unwrap();
        tx_cache.get(replacing_tx_hash).await.unwrap();
        // Both transactions have the same nonce
        assert_eq!(
            tx_cache
                .get_nonces_for_account(tx.initiator_account())
                .await,
            BTreeSet::from([Nonce(0)])
        );

        tx_cache.remove(tx_hash).await;
        assert_eq!(tx_cache.get(tx_hash).await, None);
        assert_eq!(
            tx_cache
                .get_nonces_for_account(tx.initiator_account())
                .await,
            BTreeSet::from([Nonce(0)])
        );
    }

    #[tokio::test]
    async fn transaction_is_not_stored_in_cache_on_main_node_failure() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        let params = GenesisParams::load_genesis_params(mock_genesis_config())
            .unwrap()
            .into();
        insert_genesis_batch(&mut storage, &params).await.unwrap();

        let tx = create_l2_transaction(10, 100);
        let main_node_client = MockClient::builder(L2::default())
            .method("eth_sendRawTransaction", |_bytes: Bytes| {
                Err::<H256, _>(ClientError::RequestTimeout)
            })
            .build();

        let proxy = TxProxy::new(Box::new(main_node_client));
        proxy
            .submit_tx(
                &tx,
                &SandboxExecutionOutput::mock_success(),
                ValidationTraces::default(),
            )
            .await
            .unwrap_err();

        let found_tx = proxy.find_tx(&mut storage, tx.hash()).await.unwrap();
        assert!(found_tx.is_none(), "{found_tx:?}");
    }

    #[derive(Debug, Clone, Copy)]
    enum CacheUpdateMethod {
        BackgroundTask,
        Query,
        QueryDetails,
    }

    impl CacheUpdateMethod {
        const ALL: [Self; 3] = [Self::BackgroundTask, Self::Query, Self::QueryDetails];

        async fn apply(self, pool: &ConnectionPool<Core>, proxy: &TxProxy, tx_hash: H256) {
            match self {
                CacheUpdateMethod::BackgroundTask => {
                    proxy.tx_cache.step(pool).await.unwrap();
                }
                CacheUpdateMethod::Query => {
                    let looked_up_tx = proxy
                        .lookup_tx(
                            &mut pool.connection().await.unwrap(),
                            api::TransactionId::Hash(tx_hash),
                        )
                        .await
                        .unwrap();
                    assert!(looked_up_tx.is_none());
                }
                CacheUpdateMethod::QueryDetails => {
                    let looked_up_tx = proxy
                        .lookup_tx_details(&mut pool.connection().await.unwrap(), tx_hash)
                        .await
                        .unwrap();
                    assert!(looked_up_tx.is_none());
                }
            }
        }
    }

    #[test_casing(3, CacheUpdateMethod::ALL)]
    #[tokio::test]
    async fn removing_sealed_transaction_from_cache(cache_update_method: CacheUpdateMethod) {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        let params = GenesisParams::load_genesis_params(mock_genesis_config()).unwrap();
        insert_genesis_batch(&mut storage, &params.into())
            .await
            .unwrap();

        let tx = create_l2_transaction(10, 100);
        let main_node_client = MockClient::builder(L2::default())
            .method("eth_sendRawTransaction", |_bytes: Bytes| Ok(H256::zero()))
            .build();

        // Add transaction to the cache
        let proxy = TxProxy::new(Box::new(main_node_client));
        proxy
            .submit_tx(
                &tx,
                &SandboxExecutionOutput::mock_success(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        assert_eq!(proxy.tx_cache.get(tx.hash()).await.unwrap(), tx);
        {
            let cache_inner = proxy.tx_cache.inner.read().await;
            assert!(cache_inner.transactions_by_hash.contains_key(&tx.hash()));
            assert!(cache_inner
                .nonces_by_account
                .contains_key(&tx.initiator_account()));
            assert!(cache_inner
                .tx_hashes_by_initiator
                .contains_key(&(tx.initiator_account(), Nonce(0))));
        }

        // Emulate the transaction getting sealed.
        storage
            .blocks_dal()
            .insert_l2_block(&create_l2_block(1))
            .await
            .unwrap();
        let nonce_key = get_nonce_key(&tx.initiator_account());
        let nonce_log = StorageLog::new_write_log(nonce_key, H256::from_low_u64_be(1));
        storage
            .storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(1), &[nonce_log])
            .await
            .unwrap();

        cache_update_method.apply(&pool, &proxy, tx.hash()).await;

        // Transaction should be removed from the cache
        assert!(proxy.tx_cache.get(tx.hash()).await.is_none());
        {
            let cache_inner = proxy.tx_cache.inner.read().await;
            assert!(!cache_inner.transactions_by_hash.contains_key(&tx.hash()));
            assert!(!cache_inner
                .nonces_by_account
                .contains_key(&tx.initiator_account()));
            assert!(!cache_inner
                .tx_hashes_by_initiator
                .contains_key(&(tx.initiator_account(), Nonce(0))));
        }

        let looked_up_tx = proxy
            .lookup_tx(&mut storage, api::TransactionId::Hash(tx.hash()))
            .await
            .unwrap();
        assert!(looked_up_tx.is_none());
        let looked_up_tx = proxy
            .lookup_tx_details(&mut storage, tx.hash())
            .await
            .unwrap();
        assert!(looked_up_tx.is_none());
    }

    #[test_casing(3, CacheUpdateMethod::ALL)]
    #[tokio::test]
    async fn removing_replaced_transaction_from_cache(cache_update_method: CacheUpdateMethod) {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        let params = GenesisParams::load_genesis_params(mock_genesis_config()).unwrap();
        insert_genesis_batch(&mut storage, &params.into())
            .await
            .unwrap();

        let tx = create_l2_transaction(10, 100);
        let mut replacing_tx = create_l2_transaction(10, 100);
        assert_eq!(tx.nonce(), replacing_tx.nonce());
        replacing_tx.common_data.initiator_address = tx.initiator_account();
        let mut future_tx = create_l2_transaction(10, 100);
        future_tx.common_data.initiator_address = tx.initiator_account();
        future_tx.common_data.nonce = Nonce(1);

        let main_node_client = MockClient::builder(L2::default())
            .method("eth_sendRawTransaction", |_bytes: Bytes| Ok(H256::zero()))
            .build();
        let proxy = TxProxy::new(Box::new(main_node_client));
        proxy
            .submit_tx(
                &tx,
                &SandboxExecutionOutput::mock_success(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        proxy
            .submit_tx(
                &replacing_tx,
                &SandboxExecutionOutput::mock_success(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        proxy
            .submit_tx(
                &future_tx,
                &SandboxExecutionOutput::mock_success(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        {
            let cache_inner = proxy.tx_cache.inner.read().await;
            assert_eq!(cache_inner.nonces_by_account.len(), 1);
            let account_nonces = &cache_inner.nonces_by_account[&tx.initiator_account()];
            assert_eq!(*account_nonces, BTreeSet::from([Nonce(0), Nonce(1)]));
            assert_eq!(cache_inner.tx_hashes_by_initiator.len(), 2);
            assert_eq!(
                cache_inner.tx_hashes_by_initiator[&(tx.initiator_account(), Nonce(0))],
                HashSet::from([tx.hash(), replacing_tx.hash()])
            );
            assert_eq!(
                cache_inner.tx_hashes_by_initiator[&(tx.initiator_account(), Nonce(1))],
                HashSet::from([future_tx.hash()])
            );
        }

        // Emulate the replacing transaction getting sealed.
        storage
            .blocks_dal()
            .insert_l2_block(&create_l2_block(1))
            .await
            .unwrap();
        let nonce_key = get_nonce_key(&tx.initiator_account());
        let nonce_log = StorageLog::new_write_log(nonce_key, H256::from_low_u64_be(1));
        storage
            .storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(1), &[nonce_log])
            .await
            .unwrap();

        cache_update_method
            .apply(&pool, &proxy, replacing_tx.hash())
            .await;

        // Original and replacing transactions should be removed from the cache, and the future transaction should be retained.
        {
            let cache_inner = proxy.tx_cache.inner.read().await;
            assert!(!cache_inner.transactions_by_hash.contains_key(&tx.hash()));
            assert!(!cache_inner
                .transactions_by_hash
                .contains_key(&replacing_tx.hash()));
            assert_eq!(
                cache_inner.nonces_by_account[&tx.initiator_account()],
                BTreeSet::from([Nonce(1)])
            );
            assert!(!cache_inner
                .tx_hashes_by_initiator
                .contains_key(&(tx.initiator_account(), Nonce(0))));
            assert_eq!(
                cache_inner.tx_hashes_by_initiator[&(tx.initiator_account(), Nonce(1))],
                HashSet::from([future_tx.hash()])
            );
        }

        for missing_hash in [tx.hash(), replacing_tx.hash()] {
            let looked_up_tx = proxy
                .lookup_tx(&mut storage, api::TransactionId::Hash(missing_hash))
                .await
                .unwrap();
            assert!(looked_up_tx.is_none());
            let looked_up_tx = proxy
                .lookup_tx_details(&mut storage, missing_hash)
                .await
                .unwrap();
            assert!(looked_up_tx.is_none());
        }
        proxy
            .lookup_tx(&mut storage, api::TransactionId::Hash(future_tx.hash()))
            .await
            .unwrap()
            .expect("no transaction");
        proxy
            .lookup_tx_details(&mut storage, future_tx.hash())
            .await
            .unwrap()
            .expect("no transaction");
    }
}
