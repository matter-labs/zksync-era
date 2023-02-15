use std::cmp::max;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;

use tokio::sync::RwLock;

use db_test_macro::db_test;
use zksync_dal::StorageProcessor;
use zksync_types::web3::types::{Address, BlockNumber};
use zksync_types::{
    l1::{L1Tx, OpProcessingType, PriorityQueueType},
    Execute, L1TxCommonData, Nonce, PriorityOpId, Transaction, H256, U256,
};

use super::client::Error;
use crate::eth_watch::{client::EthClient, EthWatch};

struct FakeEthClientData {
    transactions: HashMap<u64, Vec<L1Tx>>,
    last_block_number: u64,
}

impl FakeEthClientData {
    fn new() -> Self {
        Self {
            transactions: Default::default(),
            last_block_number: 0,
        }
    }

    fn add_transactions(&mut self, transactions: &[L1Tx]) {
        for transaction in transactions {
            let eth_block = transaction.eth_block();
            self.last_block_number = max(eth_block, self.last_block_number);
            self.transactions
                .entry(eth_block)
                .or_insert_with(Vec::new)
                .push(transaction.clone());
        }
    }
    fn set_last_block_number(&mut self, number: u64) {
        self.last_block_number = number;
    }
}

#[derive(Clone)]
struct FakeEthClient {
    inner: Arc<RwLock<FakeEthClientData>>,
}

impl FakeEthClient {
    fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(FakeEthClientData::new())),
        }
    }

    async fn add_transactions(&mut self, transactions: &[L1Tx]) {
        self.inner.write().await.add_transactions(transactions);
    }

    async fn set_last_block_number(&mut self, number: u64) {
        self.inner.write().await.set_last_block_number(number);
    }

    async fn block_to_number(&self, block: BlockNumber) -> u64 {
        match block {
            BlockNumber::Latest => self.inner.read().await.last_block_number,
            BlockNumber::Earliest => 0,
            BlockNumber::Pending => unreachable!(),
            BlockNumber::Number(number) => number.as_u64(),
        }
    }
}

#[async_trait::async_trait]
impl EthClient for FakeEthClient {
    async fn get_priority_op_events(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        _retries_left: usize,
    ) -> Result<Vec<L1Tx>, Error> {
        let from = self.block_to_number(from).await;
        let to = self.block_to_number(to).await;
        let mut transactions = vec![];
        for number in from..=to {
            if let Some(ops) = self.inner.read().await.transactions.get(&number) {
                transactions.extend_from_slice(ops);
            }
        }
        Ok(transactions)
    }

    async fn block_number(&self) -> Result<u64, Error> {
        Ok(self.block_to_number(BlockNumber::Latest).await)
    }

    async fn get_auth_fact(&self, _address: Address, _nonce: Nonce) -> Result<Vec<u8>, Error> {
        unreachable!()
    }

    async fn get_auth_fact_reset_time(
        &self,
        _address: Address,
        _nonce: Nonce,
    ) -> Result<u64, Error> {
        unreachable!()
    }
}

fn build_tx(serial_id: u64, eth_block: u64) -> L1Tx {
    L1Tx {
        execute: Execute {
            contract_address: Address::repeat_byte(0x11),
            calldata: vec![1, 2, 3],
            factory_deps: None,
            value: U256::zero(),
        },
        common_data: L1TxCommonData {
            serial_id: PriorityOpId(serial_id),
            sender: [1u8; 20].into(),
            deadline_block: 0,
            eth_hash: [2; 32].into(),
            eth_block,
            gas_limit: Default::default(),
            gas_per_pubdata_limit: 1u32.into(),
            full_fee: Default::default(),
            layer_2_tip_fee: U256::from(10u8),
            refund_recipient: Address::zero(),
            to_mint: Default::default(),
            priority_queue_type: PriorityQueueType::Deque,
            op_processing_type: OpProcessingType::Common,
            canonical_tx_hash: H256::from_low_u64_le(serial_id),
        },
        received_timestamp_ms: 0,
    }
}

#[db_test]
async fn test_normal_operation(connection_pool: ConnectionPool) {
    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        client.clone(),
        &connection_pool,
        5,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_test_storage().await;
    client
        .add_transactions(&[build_tx(0, 10), build_tx(1, 14), build_tx(2, 18)])
        .await;
    client.set_last_block_number(20).await;
    // second tx will not be processed, as it has less than 5 confirmations
    watcher.loop_iteration(&mut storage).await.unwrap();
    let db_txs = get_all_db_txs(&mut storage);
    assert_eq!(db_txs.len(), 2);
    let db_tx: L1Tx = db_txs[0].clone().try_into().unwrap();
    assert_eq!(db_tx.common_data.serial_id.0, 0);
    let db_tx: L1Tx = db_txs[1].clone().try_into().unwrap();
    assert_eq!(db_tx.common_data.serial_id.0, 1);

    client.set_last_block_number(25).await;
    // now the second tx will be processed
    watcher.loop_iteration(&mut storage).await.unwrap();
    let db_txs = get_all_db_txs(&mut storage);
    assert_eq!(db_txs.len(), 3);
    let db_tx: L1Tx = db_txs[2].clone().try_into().unwrap();
    assert_eq!(db_tx.common_data.serial_id.0, 2);
}

#[db_test]
#[should_panic]
async fn test_gap_in_single_batch(connection_pool: ConnectionPool) {
    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        client.clone(),
        &connection_pool,
        5,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_test_storage().await;
    client
        .add_transactions(&[
            build_tx(0, 10),
            build_tx(1, 14),
            build_tx(2, 14),
            build_tx(3, 14),
            build_tx(5, 14),
        ])
        .await;
    client.set_last_block_number(20).await;
    watcher.loop_iteration(&mut storage).await.unwrap();
}

#[db_test]
#[should_panic]
async fn test_gap_between_batches(connection_pool: ConnectionPool) {
    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        client.clone(),
        &connection_pool,
        5,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_test_storage().await;
    client
        .add_transactions(&[
            // this goes to the first batch
            build_tx(0, 10),
            build_tx(1, 14),
            build_tx(2, 14),
            // this goes to the second batch
            build_tx(4, 20),
            build_tx(5, 22),
        ])
        .await;
    client.set_last_block_number(20).await;
    watcher.loop_iteration(&mut storage).await.unwrap();
    let db_txs = get_all_db_txs(&mut storage);
    assert_eq!(db_txs.len(), 3);
    client.set_last_block_number(30).await;
    watcher.loop_iteration(&mut storage).await.unwrap();
}

#[db_test]
async fn test_overlapping_batches(connection_pool: ConnectionPool) {
    let mut client = FakeEthClient::new();
    let mut watcher = EthWatch::new(
        client.clone(),
        &connection_pool,
        5,
        std::time::Duration::from_nanos(1),
    )
    .await;

    let mut storage = connection_pool.access_test_storage().await;
    client
        .add_transactions(&[
            // this goes to the first batch
            build_tx(0, 10),
            build_tx(1, 14),
            build_tx(2, 14),
            // this goes to the second batch
            build_tx(1, 20),
            build_tx(2, 22),
            build_tx(3, 23),
            build_tx(4, 23),
        ])
        .await;
    client.set_last_block_number(20).await;
    watcher.loop_iteration(&mut storage).await.unwrap();
    let db_txs = get_all_db_txs(&mut storage);
    assert_eq!(db_txs.len(), 3);
    client.set_last_block_number(30).await;
    watcher.loop_iteration(&mut storage).await.unwrap();
    let db_txs = get_all_db_txs(&mut storage);
    assert_eq!(db_txs.len(), 5);
    let tx: L1Tx = db_txs[2].clone().try_into().unwrap();
    assert_eq!(tx.common_data.serial_id.0, 2);
    let tx: L1Tx = db_txs[4].clone().try_into().unwrap();
    assert_eq!(tx.common_data.serial_id.0, 4);
}

fn get_all_db_txs(storage: &mut StorageProcessor<'_>) -> Vec<Transaction> {
    storage.transactions_dal().reset_mempool();
    storage
        .transactions_dal()
        .sync_mempool(vec![], vec![], 0, 0, 1000)
        .0
}
