use std::collections::HashMap;
use std::future::Future;
use std::sync::RwLock;

use zksync_types::{
    api::{BlockId, Transaction, TransactionDetails, TransactionId, TransactionReceipt},
    l2::L2Tx,
    H256,
};
use zksync_web3_decl::{
    jsonrpsee::core::RpcResult,
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

/// Used by external node to proxy transaction to the main node
/// and store them while they're not synced back yet
#[derive(Debug)]
pub struct TxProxy {
    tx_cache: RwLock<HashMap<H256, L2Tx>>,
    main_node_url: String,
}

impl TxProxy {
    pub fn new(main_node_url: String) -> Self {
        Self {
            main_node_url,
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

    fn proxy_request<T, F, R>(&self, request: R) -> RpcResult<T>
    where
        T: Send,
        F: Send + Future<Output = RpcResult<T>>,
        R: 'static + Send + FnOnce(HttpClient) -> F,
    {
        let main_node_url = self.main_node_url.clone();
        crate::block_on(async move {
            // Clients are tied to the runtime they are created in, so we have to create it here.
            let client = HttpClientBuilder::default().build(&main_node_url).unwrap();
            request(client).await
        })
    }

    pub fn submit_tx(&self, tx: &L2Tx) -> RpcResult<H256> {
        let raw_tx = zksync_types::Bytes(tx.common_data.input_data().expect("raw tx is absent"));
        vlog::info!("Proxying tx {}", tx.hash());
        self.proxy_request(|client| async move { client.send_raw_transaction(raw_tx).await })
    }

    pub fn request_tx(&self, id: TransactionId) -> RpcResult<Option<Transaction>> {
        self.proxy_request(move |client| async move {
            match id {
                TransactionId::Block(BlockId::Hash(block), index) => {
                    client.get_transaction_by_block_hash_and_index(block, index)
                }
                TransactionId::Block(BlockId::Number(block), index) => {
                    client.get_transaction_by_block_number_and_index(block, index)
                }
                TransactionId::Hash(hash) => client.get_transaction_by_hash(hash),
            }
            .await
        })
    }

    pub fn request_tx_details(&self, hash: H256) -> RpcResult<Option<TransactionDetails>> {
        self.proxy_request(move |client| async move { client.get_transaction_details(hash).await })
    }

    pub fn request_tx_receipt(&self, hash: H256) -> RpcResult<Option<TransactionReceipt>> {
        self.proxy_request(move |client| async move { client.get_transaction_receipt(hash).await })
    }
}
