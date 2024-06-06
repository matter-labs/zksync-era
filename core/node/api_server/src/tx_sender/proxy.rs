use std::{
    collections::{BTreeSet, HashMap, HashSet},
    future::Future,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use chrono::{TimeZone, Utc};
use tokio::sync::{watch, RwLock};
use zksync_dal::{
    helpers::wait_for_l1_batch, transactions_dal::L2TxSubmissionResult, ConnectionPool, Core,
    CoreDal, DalError,
};
use zksync_shared_metrics::{TxStage, APP_METRICS};
use zksync_types::{api, fee::TransactionExecutionMetrics, l2::L2Tx, Address, Nonce, H256, U256};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::{ClientRpcContext, EnrichedClientResult, Web3Error},
    namespaces::EthNamespaceClient,
};

use super::{tx_sink::TxSink, SubmitTxError};

#[derive(Debug, Clone, Default)]
pub(crate) struct TxCache {
    inner: Arc<RwLock<TxCacheInner>>,
}

#[derive(Debug, Default)]
struct TxCacheInner {
    // FIXME: implement expiration / LRU?
    tx_cache: HashMap<H256, L2Tx>,
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
                        self.tx_cache.remove(&tx_hash);
                    }
                }
            }
            *account_nonces = retained_nonces;
            // If we've removed all nonces, drop the account entry so we don't request stored nonces for it later.
            !account_nonces.is_empty()
        });
    }

    /// Same as `collect_garbage()`, but optimized for a single account–nonce entry.
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
                    self.tx_cache.remove(&tx_hash);
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
        inner.tx_cache.insert(tx.hash(), tx);
    }

    async fn get_tx(&self, tx_hash: H256) -> Option<L2Tx> {
        self.inner.read().await.tx_cache.get(&tx_hash).cloned()
    }

    async fn get_nonces_for_account(&self, account_address: Address) -> BTreeSet<Nonce> {
        let inner = self.inner.read().await;
        if let Some(nonces) = inner.nonces_by_account.get(&account_address) {
            nonces.clone()
        } else {
            BTreeSet::new()
        }
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
            wait_for_l1_batch(&pool, UPDATE_INTERVAL, &mut stop_receiver)
                .await
                .context("error while waiting for L1 batch in Postgres")?;
        if let Some(number) = earliest_l1_batch_number {
            tracing::info!("Successfully waited for at least one L1 batch in Postgres; the earliest one is #{number}");
        } else {
            tracing::info!(
                "Received shutdown signal before TxCache::run_updates is started; shutting down"
            );
            return Ok(());
        }

        loop {
            if *stop_receiver.borrow() {
                return Ok(());
            }

            let addresses: Vec<_> = {
                // Split into 2 statements for readability.
                let inner = self.inner.read().await;
                inner.nonces_by_account.keys().copied().collect()
            };
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

            tokio::time::sleep(UPDATE_INTERVAL).await;
        }
    }
}

/// Used by external node to proxy transaction to the main node
/// and store them while they're not synced back yet
#[derive(Debug)]
pub struct TxProxy {
    tx_cache: TxCache,
    pool: ConnectionPool<Core>,
    client: Box<DynClient<L2>>,
}

impl TxProxy {
    pub fn new(pool: ConnectionPool<Core>, client: Box<DynClient<L2>>) -> Self {
        Self {
            tx_cache: TxCache::default(),
            pool,
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

    async fn save_tx(&self, tx: L2Tx) {
        self.tx_cache.push(tx).await;
    }

    async fn find_tx(&self, tx_hash: H256) -> Result<Option<L2Tx>, Web3Error> {
        let Some(tx) = self.tx_cache.get_tx(tx_hash).await else {
            return Ok(None);
        };

        let mut storage = self
            .pool
            .connection_tagged("api")
            .await
            .map_err(DalError::generalize)?;
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

    pub fn run_account_nonce_sweeper(
        &self,
        pool: ConnectionPool<Core>,
        stop_receiver: watch::Receiver<bool>,
    ) -> impl Future<Output = anyhow::Result<()>> {
        let tx_cache = self.tx_cache.clone();
        tx_cache.run_updates(pool, stop_receiver)
    }
}

#[async_trait::async_trait]
impl TxSink for TxProxy {
    async fn submit_tx(
        &self,
        tx: &L2Tx,
        _execution_metrics: TransactionExecutionMetrics,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        // We're running an external node: we have to proxy the transaction to the main node.
        // But before we do that, save the tx to cache in case someone will request it
        // Before it reaches the main node.
        self.save_tx(tx.clone()).await;
        self.submit_tx_impl(tx).await?;
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
        id: api::TransactionId,
    ) -> Result<Option<api::Transaction>, Web3Error> {
        if let api::TransactionId::Hash(hash) = id {
            // If the transaction is not in the db, check the cache
            if let Some(tx) = self.find_tx(hash).await? {
                // check nonce for initiator
                return Ok(Some(tx.into()));
            }
        }
        Ok(None)
    }

    async fn lookup_tx_details(
        &self,
        hash: H256,
    ) -> Result<Option<api::TransactionDetails>, Web3Error> {
        if let Some(tx) = self.find_tx(hash).await? {
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
            }));
        }
        Ok(None)
    }
}
