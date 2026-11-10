use std::collections::{hash_map, BTreeMap, BTreeSet, HashMap};

use zksync_types::{
    l1::L1Tx, l2::L2Tx, Address, ExecuteTransactionCommon, Nonce, PriorityOpId, ProtocolVersionId,
    Transaction, TransactionTimeRangeConstraint,
};

use crate::types::{AccountTransactions, AdvanceInput, L2TxFilter, MempoolScore};

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
    /// Pending L1 transactions. They are stored in a `BTreeMap` to allow fast `split_off` in `advance_after_block`.
    l1_transactions: BTreeMap<PriorityOpId, L1Tx>,
    /// Pending L2 transactions grouped by initiator address
    l2_transactions_per_account: HashMap<Address, AccountTransactions>,
    /// Global priority queue for L2 transactions. Used for scoring
    l2_priority_queue: BTreeSet<MempoolScore>,
    /// Pending high priority L2 transactions grouped by initiator address
    high_priority_l2_transactions_per_account: HashMap<Address, AccountTransactions>,
    /// Global priority queue for high priority L2 transactions. Used for scoring
    high_priority_l2_priority_queue: BTreeSet<MempoolScore>,
    /// Address of the initiator of high priority L2 transactions.
    high_priority_l2_tx_initiator: Option<Address>,
    /// Protocol version from which the high priority L2 transactions are allowed and prioritized.
    high_priority_l2_tx_protocol_version: Option<ProtocolVersionId>,
    /// Next priority operation
    next_priority_id: PriorityOpId,
    /// Whether L1 (priority) transactions are withheld from execution. While set, `next_transaction`
    /// never returns one and `has_next` ignores them, no matter how many are buffered.
    ///
    /// This starts from `mempool.l1_to_l2_txs_paused` and is also latched at run time when the state
    /// keeper finds a priority transaction unexecutable; see `MempoolStore::pause_l1_transactions`.
    l1_transactions_paused: bool,
    stashed_accounts: Vec<Address>,
    /// Number of L2 transactions in the mempool.
    size: u64,
    capacity: u64,
}

impl MempoolStore {
    pub fn new(
        next_priority_id: PriorityOpId,
        capacity: u64,
        high_priority_l2_tx_initiator: Option<Address>,
        high_priority_l2_tx_protocol_version: Option<ProtocolVersionId>,
    ) -> Self {
        Self {
            l1_transactions: BTreeMap::new(),
            l2_transactions_per_account: HashMap::new(),
            l2_priority_queue: BTreeSet::new(),
            high_priority_l2_transactions_per_account: HashMap::new(),
            high_priority_l2_priority_queue: BTreeSet::new(),
            high_priority_l2_tx_initiator,
            high_priority_l2_tx_protocol_version,
            next_priority_id,
            l1_transactions_paused: false,
            stashed_accounts: vec![],
            size: 0,
            capacity,
        }
    }

    /// Withholds every L1 (priority) transaction from execution, from now on.
    ///
    /// This is a latch: there is no way to resume within the run of a node. Resuming means restarting
    /// with `mempool.l1_to_l2_txs_paused` unset, which is deliberate — whatever made the state keeper
    /// pause has to be dealt with by an operator, and a priority transaction that was withheld once
    /// would otherwise be retried on the very next block.
    ///
    /// Buffered priority transactions are kept rather than dropped. Nothing is lost by pausing: an
    /// L1 transaction only leaves the mempool for good once it is executed in a sealed batch, so a
    /// paused node resumes where it stopped after a restart.
    pub fn pause_l1_transactions(&mut self) {
        self.l1_transactions_paused = true;
    }

    /// Whether L1 (priority) transactions are currently withheld from execution.
    pub fn l1_transactions_paused(&self) -> bool {
        self.l1_transactions_paused
    }

    /// Inserts batch of new transactions to mempool
    /// `initial_nonces` provides current committed nonce information to mempool
    /// variable is used only if account is not present in mempool yet and we have to bootstrap it
    /// in other cases mempool relies on state keeper and its internal state to keep that info up to date
    pub fn insert(
        &mut self,
        transactions: Vec<(Transaction, TransactionTimeRangeConstraint)>,
        initial_nonces: HashMap<Address, Nonce>,
    ) {
        for (transaction, constraint) in transactions {
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
                        constraint,
                        &initial_nonces,
                    );
                }
                ExecuteTransactionCommon::ProtocolUpgrade(_) => {
                    panic!("Protocol upgrade tx is not supposed to be inserted into mempool");
                }
            }
        }
    }

    #[cfg(test)]
    pub fn insert_without_constraints(
        &mut self,
        transactions: Vec<Transaction>,
        initial_nonces: HashMap<Address, Nonce>,
    ) {
        self.insert(
            transactions
                .into_iter()
                .map(|x| (x, TransactionTimeRangeConstraint::default()))
                .collect(),
            initial_nonces,
        );
    }

    fn insert_l2_transaction(
        &mut self,
        transaction: L2Tx,
        constraint: TransactionTimeRangeConstraint,
        initial_nonces: &HashMap<Address, Nonce>,
    ) {
        let account = transaction.initiator_account();

        let (txs_per_account, priority_queue) =
            if Some(account) == self.high_priority_l2_tx_initiator {
                (
                    &mut self.high_priority_l2_transactions_per_account,
                    &mut self.high_priority_l2_priority_queue,
                )
            } else {
                (
                    &mut self.l2_transactions_per_account,
                    &mut self.l2_priority_queue,
                )
            };

        let metadata = match txs_per_account.entry(account) {
            hash_map::Entry::Occupied(mut txs) => txs.get_mut().insert(transaction, constraint),
            hash_map::Entry::Vacant(entry) => {
                let account_nonce = initial_nonces.get(&account).cloned().unwrap_or(Nonce(0));
                entry
                    .insert(AccountTransactions::new(account_nonce))
                    .insert(transaction, constraint)
            }
        };

        if let Some(score) = metadata.previous_score {
            priority_queue.remove(&score);
        }
        if let Some(score) = metadata.new_score {
            priority_queue.insert(score);
        }
        if metadata.is_new {
            self.size += 1;
        }
    }

    /// Returns `true` if there is a transaction in the mempool satisfying the filter.
    pub fn has_next(&self, filter: &L2TxFilter) -> bool {
        let has_priority_tx =
            if let Some(protocol_version) = self.high_priority_l2_tx_protocol_version {
                filter.protocol_version >= protocol_version
                    && self
                        .high_priority_l2_priority_queue
                        .iter()
                        .rfind(|el| el.matches_filter(filter))
                        .is_some()
            } else {
                false
            };

        let has_l1_tx = !self.l1_transactions_paused
            && self.l1_transactions.contains_key(&self.next_priority_id);

        has_l1_tx
            || self
                .l2_priority_queue
                .iter()
                .rfind(|el| el.matches_filter(filter))
                .is_some()
            || has_priority_tx
    }

    /// Returns next transaction for execution from mempool
    pub fn next_transaction(
        &mut self,
        filter: &L2TxFilter,
    ) -> Option<(Transaction, TransactionTimeRangeConstraint)> {
        if let Some(transaction_data) = self.next_l2_transaction(filter, true) {
            return Some(transaction_data);
        }

        // While paused, priority transactions stay buffered and are never handed out, so the state
        // keeper builds batches out of L2 transactions alone.
        if !self.l1_transactions_paused {
            if let Some(transaction) = self.l1_transactions.remove(&self.next_priority_id) {
                self.next_priority_id += 1;
                // L1 transactions can't use block.timestamp in AA and hence do not need to have a constraint
                return Some((
                    transaction.into(),
                    TransactionTimeRangeConstraint::default(),
                ));
            }
        }

        self.next_l2_transaction(filter, false)
    }

    fn next_l2_transaction(
        &mut self,
        filter: &L2TxFilter,
        is_high_priority: bool,
    ) -> Option<(Transaction, TransactionTimeRangeConstraint)> {
        if is_high_priority && filter.protocol_version < self.high_priority_l2_tx_protocol_version?
        {
            return None;
        }

        let (txs_per_account, priority_queue) = if is_high_priority {
            (
                &mut self.high_priority_l2_transactions_per_account,
                &mut self.high_priority_l2_priority_queue,
            )
        } else {
            (
                &mut self.l2_transactions_per_account,
                &mut self.l2_priority_queue,
            )
        };

        let mut removed = 0;
        // We want to fetch the next transaction that would match the fee requirements.
        let tx_pointer = priority_queue
            .iter()
            .rfind(|el| el.matches_filter(filter))?
            .clone();

        let initial_length = self.stashed_accounts.len();

        // Stash all observed transactions that don't meet criteria
        for stashed_pointer in priority_queue.split_off(&tx_pointer).into_iter().skip(1) {
            removed += {
                let account = txs_per_account
                    .get_mut(&stashed_pointer.account)
                    .expect("mempool: dangling pointer in priority queue");
                let txs_len = account.len();
                account.clear_txs();
                txs_len
            };

            self.stashed_accounts.push(stashed_pointer.account);
        }

        tracing::trace!(
            "Stashed {} accounts by filter: {:?}",
            self.stashed_accounts.len() - initial_length,
            filter
        );

        // insert pointer to the next transaction if it exists
        let (transaction, constraint, score) = txs_per_account
            .get_mut(&tx_pointer.account)
            .expect("mempool: dangling pointer in priority queue")
            .next();

        if let Some(score) = score {
            priority_queue.insert(score);
        }
        self.size = self
            .size
            .checked_sub((removed + 1) as u64)
            .expect("mempool size can't be negative");

        Some((transaction.into(), constraint))
    }

    /// When a state_keeper starts the block over after a rejected transaction,
    /// we have to rollback the nonces/ids in the mempool and
    /// reinsert the transactions from the block back into mempool.
    ///
    /// Returns `None` if the transaction's account is no longer in the mempool (e.g., purged by gc);
    /// such a transaction must not be reinserted.
    pub fn rollback(&mut self, tx: &Transaction) -> Option<TransactionTimeRangeConstraint> {
        // rolling back the nonces and priority ids
        match &tx.common_data {
            ExecuteTransactionCommon::L1(data) => {
                // reset next priority id
                self.next_priority_id = self.next_priority_id.min(data.serial_id);
                Some(TransactionTimeRangeConstraint::default())
            }
            ExecuteTransactionCommon::L2(_) => {
                let account = tx.initiator_account();
                let (txs_per_account, priority_queue) =
                    if Some(account) == self.high_priority_l2_tx_initiator {
                        (
                            &mut self.high_priority_l2_transactions_per_account,
                            &mut self.high_priority_l2_priority_queue,
                        )
                    } else {
                        (
                            &mut self.l2_transactions_per_account,
                            &mut self.l2_priority_queue,
                        )
                    };

                let Some(account_txs) = txs_per_account.get_mut(&account) else {
                    tracing::warn!(
                        "not reinserting rolled back tx {:?}: account {account:?} is not in mempool",
                        tx.hash()
                    );
                    return None;
                };
                if let Some((score, constraint)) = account_txs.reset(tx) {
                    priority_queue.remove(&score);
                    return Some(constraint);
                }
                Some(TransactionTimeRangeConstraint::default())
            }
            ExecuteTransactionCommon::ProtocolUpgrade(_) => {
                panic!("Protocol upgrade tx is not supposed to be in mempool");
            }
        }
    }

    /// Advances mempool state after processed block, i.e. updates `next_priority_id` and next nonces for accounts.
    pub fn advance_after_block(&mut self, input: AdvanceInput) {
        if let Some(next_priority_id) = input.next_priority_id {
            self.next_priority_id = self.next_priority_id.max(next_priority_id);
            self.l1_transactions = self.l1_transactions.split_off(&self.next_priority_id);
        }

        for (address, nonce) in input.next_account_nonces {
            if let Some(account_txs) = self.l2_transactions_per_account.get_mut(&address) {
                // Drop processed txs and update priority queue.
                let metadata = account_txs.advance(nonce);
                if let Some(score) = metadata.previous_score {
                    self.l2_priority_queue.remove(&score);
                }
                if let Some(score) = metadata.new_score {
                    self.l2_priority_queue.insert(score);
                }
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
        if self.size > self.capacity {
            let priority_queue_and_txs_per_account = vec![
                (
                    &mut self.l2_priority_queue,
                    &mut self.l2_transactions_per_account,
                ),
                (
                    &mut self.high_priority_l2_priority_queue,
                    &mut self.high_priority_l2_transactions_per_account,
                ),
            ];

            let mut all_drained = vec![];
            self.size = 0;
            for (priority_queue, txs_per_account) in priority_queue_and_txs_per_account {
                let mut transactions = std::mem::take(txs_per_account);
                let mut possibly_kept: Vec<_> = priority_queue
                    .iter()
                    .rev()
                    .filter_map(|pointer| {
                        transactions
                            .remove(&pointer.account)
                            .map(|txs| (pointer.account, txs))
                    })
                    .collect();

                let mut sum = 0;
                let mut number_of_accounts_kept = 0;
                for (_, txs) in &possibly_kept {
                    sum += txs.len();
                    if sum <= self.capacity as usize {
                        number_of_accounts_kept += 1;
                    } else {
                        break;
                    }
                }
                if number_of_accounts_kept == 0 && !possibly_kept.is_empty() {
                    tracing::warn!("mempool capacity is too low to handle txs from single account, consider increasing capacity");
                    // Keep at least one entry, otherwise mempool won't return any new L2 tx to process.
                    number_of_accounts_kept = 1;
                }
                let (kept, drained) = {
                    let mut drained: Vec<_> = transactions.into_keys().collect();
                    let also_drained = possibly_kept
                        .split_off(number_of_accounts_kept)
                        .into_iter()
                        .map(|(address, _)| address);
                    drained.extend(also_drained);

                    (possibly_kept, drained)
                };

                let new_priority_queue = std::mem::take(priority_queue);
                *priority_queue = new_priority_queue
                    .into_iter()
                    .rev()
                    .take(number_of_accounts_kept)
                    .collect();
                *txs_per_account = kept.into_iter().collect();
                self.size += txs_per_account
                    .iter()
                    .fold(0, |agg, (_, txs)| agg + txs.len() as u64);

                all_drained.extend(drained);
            }

            return all_drained;
        }
        vec![]
    }

    pub fn account_nonce(&self, address: Address) -> Option<Nonce> {
        self.l2_transactions_per_account
            .get(&address)
            .map(|a| a.nonce())
    }
}

#[cfg(test)]
impl MempoolStore {
    pub fn next_priority_id(&self) -> PriorityOpId {
        self.next_priority_id
    }

    pub fn l1_transactions(&self) -> &BTreeMap<PriorityOpId, L1Tx> {
        &self.l1_transactions
    }

    pub fn l2_priority_queue(&self) -> &BTreeSet<MempoolScore> {
        &self.l2_priority_queue
    }
}
