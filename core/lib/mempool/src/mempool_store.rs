use std::collections::{hash_map, BTreeSet, HashMap, HashSet};

use crate::types::{AccountTransactions, L2TxFilter, MempoolScore};
use zksync_types::{
    l1::L1Tx, l2::L2Tx, Address, ExecuteTransactionCommon, Nonce, PriorityOpId, Transaction,
};

#[derive(Debug)]
pub struct MempoolInfo {
    pub stashed_accounts: Vec<Address>,
    pub purged_accounts: Vec<Address>,
}

#[derive(Debug)]
pub struct MempoolStats {
    pub l1_transaction_count: usize,
    pub l2_transaction_count: u64,
    pub l2_priority_queue_size: usize,
}

#[derive(Debug)]
pub struct MempoolStore {
    /// Pending L1 transactions
    l1_transactions: HashMap<PriorityOpId, L1Tx>,
    /// Pending L2 transactions grouped by initiator address
    l2_transactions_per_account: HashMap<Address, AccountTransactions>,
    /// Global priority queue for L2 transactions. Used for scoring
    l2_priority_queue: BTreeSet<MempoolScore>,
    /// Next priority operation
    next_priority_id: PriorityOpId,
    stashed_accounts: Vec<Address>,
    /// Number of l2 transactions in the mempool.
    size: u64,
    capacity: u64,
}

impl MempoolStore {
    pub fn new(next_priority_id: PriorityOpId, capacity: u64) -> Self {
        Self {
            l1_transactions: HashMap::new(),
            l2_transactions_per_account: HashMap::new(),
            l2_priority_queue: BTreeSet::new(),
            next_priority_id,
            stashed_accounts: vec![],
            size: 0,
            capacity,
        }
    }

    /// Inserts batch of new transactions to mempool
    /// `initial_nonces` provides current committed nonce information to mempool
    /// variable is used only if account is not present in mempool yet and we have to bootstrap it
    /// in other cases mempool relies on state keeper and its internal state to keep that info up to date
    pub fn insert(
        &mut self,
        transactions: Vec<Transaction>,
        initial_nonces: HashMap<Address, Nonce>,
    ) {
        for transaction in transactions {
            let Transaction {
                common_data,
                execute,
                received_timestamp_ms,
                raw_bytes,
            } = transaction;
            match common_data {
                ExecuteTransactionCommon::L1(data) => {
                    tracing::trace!("inserting L1 transaction {}", data.serial_id);
                    self.l1_transactions.insert(
                        data.serial_id,
                        L1Tx {
                            execute,
                            common_data: data,
                            received_timestamp_ms,
                        },
                    );
                }
                ExecuteTransactionCommon::L2(data) => {
                    tracing::trace!("inserting L2 transaction {}", data.nonce);
                    self.insert_l2_transaction(
                        L2Tx {
                            execute,
                            common_data: data,
                            received_timestamp_ms,
                            raw_bytes,
                        },
                        &initial_nonces,
                    );
                }
                ExecuteTransactionCommon::ProtocolUpgrade(_) => {
                    panic!("Protocol upgrade tx is not supposed to be inserted into mempool");
                }
            }
        }
    }

    fn insert_l2_transaction(
        &mut self,
        transaction: L2Tx,
        initial_nonces: &HashMap<Address, Nonce>,
    ) {
        let account = transaction.initiator_account();

        let metadata = match self.l2_transactions_per_account.entry(account) {
            hash_map::Entry::Occupied(mut txs) => txs.get_mut().insert(transaction),
            hash_map::Entry::Vacant(entry) => {
                let account_nonce = initial_nonces.get(&account).cloned().unwrap_or(Nonce(0));
                entry
                    .insert(AccountTransactions::new(account_nonce))
                    .insert(transaction)
            }
        };
        if let Some(score) = metadata.previous_score {
            self.l2_priority_queue.remove(&score);
        }
        if let Some(score) = metadata.new_score {
            self.l2_priority_queue.insert(score);
        }
        if metadata.is_new {
            self.size += 1;
        }
    }

    /// Returns `true` if there is a transaction in the mempool satisfying the filter.
    pub fn has_next(&self, filter: &L2TxFilter) -> bool {
        self.l1_transactions.get(&self.next_priority_id).is_some()
            || self
                .l2_priority_queue
                .iter()
                .rfind(|el| el.matches_filter(filter))
                .is_some()
    }

    /// Returns next transaction for execution from mempool
    pub fn next_transaction(&mut self, filter: &L2TxFilter) -> Option<Transaction> {
        if let Some(transaction) = self.l1_transactions.remove(&self.next_priority_id) {
            self.next_priority_id += 1;
            return Some(transaction.into());
        }

        let mut removed = 0;
        // We want to fetch the next transaction that would match the fee requirements.
        let tx_pointer = self
            .l2_priority_queue
            .iter()
            .rfind(|el| el.matches_filter(filter))?
            .clone();

        // Stash all observed transactions that don't meet criteria
        for stashed_pointer in self
            .l2_priority_queue
            .split_off(&tx_pointer)
            .into_iter()
            .skip(1)
        {
            removed += self
                .l2_transactions_per_account
                .remove(&stashed_pointer.account)
                .expect("mempool: dangling pointer in priority queue")
                .len();

            self.stashed_accounts.push(stashed_pointer.account);
        }
        // insert pointer to the next transaction if it exists
        let (transaction, score) = self
            .l2_transactions_per_account
            .get_mut(&tx_pointer.account)
            .expect("mempool: dangling pointer in priority queue")
            .next();

        if let Some(score) = score {
            self.l2_priority_queue.insert(score);
        }
        self.size = self
            .size
            .checked_sub((removed + 1) as u64)
            .expect("mempool size can't be negative");
        Some(transaction.into())
    }

    /// When a state_keeper starts the block over after a rejected transaction,
    /// we have to rollback the nonces/ids in the mempool and
    /// reinsert the transactions from the block back into mempool.
    pub fn rollback(&mut self, tx: &Transaction) {
        // rolling back the nonces and priority ids
        match &tx.common_data {
            ExecuteTransactionCommon::L1(data) => {
                // reset next priority id
                self.next_priority_id = self.next_priority_id.min(data.serial_id);
            }
            ExecuteTransactionCommon::L2(_) => {
                if let Some(score) = self
                    .l2_transactions_per_account
                    .get_mut(&tx.initiator_account())
                    .expect("account is not available in mempool")
                    .reset(tx)
                {
                    self.l2_priority_queue.remove(&score);
                }
            }
            ExecuteTransactionCommon::ProtocolUpgrade(_) => {
                panic!("Protocol upgrade tx is not supposed to be in mempool");
            }
        }
    }

    pub fn get_mempool_info(&mut self) -> MempoolInfo {
        MempoolInfo {
            stashed_accounts: std::mem::take(&mut self.stashed_accounts),
            purged_accounts: self.gc(),
        }
    }

    pub fn stats(&self) -> MempoolStats {
        MempoolStats {
            l1_transaction_count: self.l1_transactions.len(),
            l2_transaction_count: self.size,
            l2_priority_queue_size: self.l2_priority_queue.len(),
        }
    }

    fn gc(&mut self) -> Vec<Address> {
        if self.size >= self.capacity {
            let index: HashSet<_> = self
                .l2_priority_queue
                .iter()
                .map(|pointer| pointer.account)
                .collect();
            let transactions = std::mem::take(&mut self.l2_transactions_per_account);
            let (kept, drained) = transactions
                .into_iter()
                .partition(|(address, _)| index.contains(address));
            self.l2_transactions_per_account = kept;
            self.size = self
                .l2_transactions_per_account
                .iter()
                .fold(0, |agg, (_, tnxs)| agg + tnxs.len() as u64);
            return drained.into_keys().collect();
        }
        vec![]
    }
}
