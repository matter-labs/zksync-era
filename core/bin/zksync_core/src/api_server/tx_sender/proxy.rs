use std::collections::HashMap;
use std::sync::RwLock;

use zksync_types::{
    api::{BlockId, Transaction, TransactionId},
    l2::L2Tx,
    H256,
};
use zksync_web3_decl::{
    jsonrpsee::core::Error as JsonrpseeError,
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::EthNamespaceClient,
};

/// Used by external node to proxy transaction to the main node
/// and store them while they're not synced back yet
pub struct TxProxy {
    tx_cache: RwLock<HashMap<H256, L2Tx>>,
    client: HttpClient,
}

impl TxProxy {
    pub fn new(main_node_url: &str) -> Self {
        Self {
            client: HttpClientBuilder::default()
                .build(main_node_url)
                .expect("Failed to create HTTP client"),
            tx_cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn find_tx(&self, tx_hash: H256) -> Option<L2Tx> {
        self.tx_cache.read().unwrap().get(&tx_hash).cloned()
    }

    pub fn forget_tx(&self, tx_hash: H256) {
        self.tx_cache.write().unwrap().remove(&tx_hash);
    }

    pub fn save_tx(&self, tx_hash: H256, tx: L2Tx) {
        self.tx_cache.write().unwrap().insert(tx_hash, tx);
    }

    pub fn submit_tx(&self, tx: &L2Tx) -> Result<H256, JsonrpseeError> {
        let raw_tx = zksync_types::Bytes(tx.common_data.input_data().expect("raw tx is absent"));
        async_std::task::block_on(self.client.send_raw_transaction(raw_tx))
    }

    pub fn request_tx(&self, id: TransactionId) -> Result<Option<Transaction>, JsonrpseeError> {
        async_std::task::block_on(match id {
            TransactionId::Block(BlockId::Hash(block), index) => self
                .client
                .get_transaction_by_block_hash_and_index(block, index),
            TransactionId::Block(BlockId::Number(block), index) => self
                .client
                .get_transaction_by_block_number_and_index(block, index),
            TransactionId::Hash(hash) => self.client.get_transaction_by_hash(hash),
        })
    }
}
