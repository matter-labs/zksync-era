use std::{collections::HashMap, sync::Arc};

use itertools::Itertools;
use tokio::sync::RwLock;
use zksync_types::{
    api::{BlockId, Transaction, TransactionDetails, TransactionId},
    l2::L2Tx,
    Address, H256,
};
use zksync_web3_decl::{
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
    RpcResult,
};

#[derive(Debug, Default)]
pub struct TxCache {
    tx_cache: HashMap<H256, L2Tx>,
    tx_cache_by_account: HashMap<Address, Vec<H256>>,
}

impl TxCache {
    fn push(&mut self, tx_hash: H256, tx: L2Tx) {
        let account_address = tx.common_data.initiator_address;
        self.tx_cache.insert(tx_hash, tx);
        self.tx_cache_by_account
            .entry(account_address)
            .or_default()
            .push(tx_hash);
    }

    fn get_tx(&self, tx_hash: &H256) -> Option<L2Tx> {
        self.tx_cache.get(tx_hash).cloned()
    }

    fn get_txs_by_account(&self, account_address: Address) -> Vec<L2Tx> {
        let Some(tx_hashes) = self.tx_cache_by_account.get(&account_address) else {
            return Vec::new();
        };

        let mut txs = Vec::new();
        for tx_hash in tx_hashes {
            if let Some(tx) = self.get_tx(tx_hash) {
                txs.push(tx);
            }
        }
        txs.into_iter().sorted_by_key(|tx| tx.nonce()).collect()
    }

    pub(crate) fn remove_tx(&mut self, tx_hash: &H256) {
        let tx = self.tx_cache.remove(tx_hash);
        if let Some(tx) = tx {
            let account_tx_hashes = self
                .tx_cache_by_account
                .get_mut(&tx.common_data.initiator_address);
            if let Some(account_tx_hashes) = account_tx_hashes {
                account_tx_hashes.retain(|&hash| hash != *tx_hash);
            }
        }
    }
}

/// Used by external node to proxy transaction to the main node
/// and store them while they're not synced back yet
#[derive(Debug)]
pub struct TxProxy {
    tx_cache: Arc<RwLock<TxCache>>,
    client: HttpClient,
}

impl TxProxy {
    pub fn new(main_node_url: &str, tx_cache: Arc<RwLock<TxCache>>) -> Self {
        let client = HttpClientBuilder::default().build(main_node_url).unwrap();
        Self { client, tx_cache }
    }

    pub async fn find_tx(&self, tx_hash: H256) -> Option<L2Tx> {
        self.tx_cache.read().await.get_tx(&tx_hash)
    }

    pub async fn forget_tx(&self, tx_hash: H256) {
        self.tx_cache.write().await.remove_tx(&tx_hash)
    }

    pub async fn save_tx(&self, tx_hash: H256, tx: L2Tx) {
        self.tx_cache.write().await.push(tx_hash, tx)
    }

    pub async fn get_txs_by_account(&self, account_address: Address) -> Vec<L2Tx> {
        self.tx_cache
            .read()
            .await
            .get_txs_by_account(account_address)
    }

    pub async fn submit_tx(&self, tx: &L2Tx) -> RpcResult<H256> {
        let input_data = tx.common_data.input_data().expect("raw tx is absent");
        let raw_tx = zksync_types::Bytes(input_data.to_vec());
        tracing::info!("Proxying tx {}", tx.hash());
        self.client.send_raw_transaction(raw_tx).await
    }

    pub async fn request_tx(&self, id: TransactionId) -> RpcResult<Option<Transaction>> {
        match id {
            TransactionId::Block(BlockId::Hash(block), index) => {
                self.client
                    .get_transaction_by_block_hash_and_index(block, index)
                    .await
            }
            TransactionId::Block(BlockId::Number(block), index) => {
                self.client
                    .get_transaction_by_block_number_and_index(block, index)
                    .await
            }
            TransactionId::Hash(hash) => self.client.get_transaction_by_hash(hash).await,
        }
    }

    pub async fn request_tx_details(&self, hash: H256) -> RpcResult<Option<TransactionDetails>> {
        self.client.get_transaction_details(hash).await
    }
}
