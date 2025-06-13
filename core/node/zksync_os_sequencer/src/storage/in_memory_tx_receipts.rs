use std::sync::Arc;
use dashmap::DashMap;
use zk_ee::utils::Bytes32;
use zksync_types::api;

#[derive(Clone, Debug)]
pub struct TransactionApiData {
    pub transaction: api::Transaction,
    pub receipt: api::TransactionReceipt,
}
/// Thread-safe in-memory store of transaction receipts, keyed by transaction hash.
///
/// Retains all inserted receipts indefinitely. Internally uses a lock-free
/// DashMap to allow concurrent inserts and lookups.
#[derive(Clone, Debug)]
pub struct InMemoryTxReceipts {
    /// Map from tx hash → receipt
    receipts: Arc<DashMap<Bytes32, TransactionApiData>>,
}

impl InMemoryTxReceipts {
    /// Creates an empty store.
    pub fn empty() -> Self {
        InMemoryTxReceipts {
            receipts: Arc::new(DashMap::new()),
        }
    }

    /// Inserts a receipt for `tx_hash`. If a receipt for the same hash
    /// already exists, it will be overwritten.
    pub fn insert(&self, tx_hash: Bytes32, data: TransactionApiData) {
        self.receipts.insert(tx_hash, data);
    }

    /// Retrieves the receipt for `tx_hash`, if present.
    /// Returns a cloned `TransactionReceipt`.
    pub fn get(&self, tx_hash: &Bytes32) -> Option<TransactionApiData> {
        self.receipts.get(tx_hash).map(|r| r.value().clone())
    }

    /// Returns `true` if a receipt for `tx_hash` is present.
    pub fn contains(&self, tx_hash: &Bytes32) -> bool {
        self.receipts.contains_key(tx_hash)
    }

    /// Returns the total number of receipts stored.
    pub fn len(&self) -> usize {
        self.receipts.len()
    }

    /// Returns whether the store is currently empty.
    pub fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }
}
