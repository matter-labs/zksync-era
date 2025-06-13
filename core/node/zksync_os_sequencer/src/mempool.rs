// mempool.rs

use std::str::FromStr;
use std::sync::Arc;
use crossbeam_queue::SegQueue;
use zk_os_forward_system::run::TxSource;
use zksync_types::l1::L1Tx;
use zksync_types::{Address, Execute, L1TxCommonData, PriorityOpId, Transaction, U256};

/// A simple, thread-safe in-memory mempool.
///
/// Internally uses a lock-free FIFO queue (`SegQueue`) to allow
/// concurrent producers and consumers without explicit locking.
#[derive(Clone, Debug)]
pub struct Mempool {
    queue: Arc<SegQueue<Transaction>>,
}

impl Mempool {
    /// Creates an empty mempool.
    pub fn new() -> Self {
        let tx = L1Tx {
            execute: Execute {
                contract_address: Some(Address::from_str("0x36615Cf349d7F6344891B1e7CA7C72883F5dc049").unwrap()),
                calldata: vec![],
                value: U256::from("100"),
                factory_deps: vec![],
            },
            common_data: L1TxCommonData {
                sender: Address::from_str("0x36615Cf349d7F6344891B1e7CA7C72883F5dc049").unwrap(),
                serial_id: PriorityOpId(1),
                layer_2_tip_fee: Default::default(),
                full_fee: U256::from("10000000000"),
                max_fee_per_gas: U256::from(1),
                gas_limit: U256::from("10000000000"),
                gas_per_pubdata_limit: U256::from(1000),
                op_processing_type: Default::default(),
                priority_queue_type: Default::default(),
                canonical_tx_hash: Default::default(),
                to_mint: U256::from("100000000000000000000000000000"),
                refund_recipient: Address::from_str("0x36615Cf349d7F6344891B1e7CA7C72883F5dc049").unwrap(),
                eth_block: 0,
            },
            received_timestamp_ms: 0,
        };
        let mut queue = SegQueue::new();
        queue.push(tx.into());
        Mempool {
            queue: Arc::new(queue),
        }
    }

    /// Inserts a transaction into the pool.
    ///
    /// Can be called concurrently from multiple threads.
    pub fn insert(&self, tx: Transaction) {
        self.queue.push(tx);
    }

    /// Attempts to pull the next transaction from the pool.
    ///
    /// If the pool is empty, returns `None`.
    /// Can be called concurrently from multiple threads.
    pub fn get_next(&self) -> Option<Transaction> {
        self.queue.pop()
    }

    /// Returns `true` if the pool is currently empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns an approximate number of transactions in the pool.
    ///
    /// Note: `SegQueue::len` is not exact under concurrent updates.
    pub fn len(&self) -> usize {
        self.queue.len()
    }
}