use std::collections::HashMap;
use tokio::sync::RwLock;

use zksync_types::{
    api::{BlockId, Transaction, TransactionDetails, TransactionId, TransactionReceipt},
    l2::L2Tx,
    H256,
};
use zksync_web3_decl::{
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
    RpcResult,
};

/// Used by external node to proxy transaction to the main node
/// and store them while they're not synced back yet
#[derive(Debug)]
pub struct TxProxy {
    tx_cache: RwLock<HashMap<H256, L2Tx>>,
    client: HttpClient,
}

impl TxProxy {
    pub fn new(main_node_url: &str) -> Self {
        let client = HttpClientBuilder::default().build(main_node_url).unwrap();
        Self {
            client,
            tx_cache: RwLock::new(HashMap::new()),
        }
    }

    pub async fn find_tx(&self, tx_hash: H256) -> Option<L2Tx> {
        self.tx_cache.read().await.get(&tx_hash).cloned()
    }

    pub async fn forget_tx(&self, tx_hash: H256) {
        self.tx_cache.write().await.remove(&tx_hash);
    }

    pub async fn save_tx(&self, tx_hash: H256, tx: L2Tx) {
        self.tx_cache.write().await.insert(tx_hash, tx);
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

    pub async fn request_tx_receipt(&self, hash: H256) -> RpcResult<Option<TransactionReceipt>> {
        self.client.get_transaction_receipt(hash).await
    }
}
