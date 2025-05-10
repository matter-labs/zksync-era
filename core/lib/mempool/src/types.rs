use std::{cmp::Ordering, collections::HashMap};

use zksync_types::{
    fee::Fee, fee_model::BatchFeeInput, l2::L2Tx, Address, Nonce, Transaction,
    TransactionTimeRangeConstraint, U256,
};

/// Pending mempool transactions of account
#[derive(Debug)]
pub(crate) struct AccountTransactions {
    /// transactions that belong to given account keyed by transaction nonce
    transactions: HashMap<Nonce, (L2Tx, TransactionTimeRangeConstraint)>,
    /// account nonce in mempool
    /// equals to committed nonce in db + number of transactions sent to state keeper
    nonce: Nonce,
}

impl AccountTransactions {
    pub fn new(nonce: Nonce) -> Self {
        Self {
            transactions: HashMap::new(),
            nonce,
        }
    }

    /// Inserts new transaction for given account. Returns insertion metadata
    pub fn insert(
        &mut self,
        transaction: L2Tx,
        constraint: TransactionTimeRangeConstraint,
    ) -> InsertionMetadata {
        let mut metadata = InsertionMetadata::default();
        let nonce = transaction.common_data.nonce;
        // skip insertion if transaction is old
        if nonce < self.nonce {
            return metadata;
        }
        let new_score = Self::score_for_transaction(&transaction);
        let previous_score = self
            .transactions
            .insert(nonce, (transaction, constraint))
            .map(|x| Self::score_for_transaction(&x.0));
        metadata.is_new = previous_score.is_none();
        if nonce == self.nonce {
            metadata.new_score = Some(new_score);
            metadata.previous_score = previous_score;
        }
        metadata
    }

    /// Returns next transaction to be included in block, its time range constraint and optional
    /// score of its successor. Panics if no such transaction exists
    pub fn next(&mut self) -> (L2Tx, TransactionTimeRangeConstraint, Option<MempoolScore>) {
        let transaction = self
            .transactions
            .remove(&self.nonce)
            .expect("missing transaction in mempool");
        self.nonce += 1;
        let score = self
            .transactions
            .get(&self.nonce)
            .map(|(tx, _c)| Self::score_for_transaction(tx));
        (transaction.0, transaction.1, score)
    }

    /// Handles transaction rejection. Returns optional score of its successor and time range
    /// constraint that the transaction has been added to the mempool with
    pub fn reset(
        &mut self,
        transaction: &Transaction,
    ) -> Option<(MempoolScore, TransactionTimeRangeConstraint)> {
        // current nonce for the group needs to be reset
        let tx_nonce = transaction
            .nonce()
            .expect("nonce is not set for L2 transaction");
        self.nonce = self.nonce.min(tx_nonce);
        self.transactions
            .get(&(tx_nonce + 1))
            .map(|(tx, c)| (Self::score_for_transaction(tx), c.clone()))
    }

    pub fn len(&self) -> usize {
        self.transactions.len()
    }

    pub fn nonce(&self) -> Nonce {
        self.nonce
    }

    pub fn clear_txs(&mut self) {
        self.transactions.clear();
    }

    fn score_for_transaction(transaction: &L2Tx) -> MempoolScore {
        MempoolScore {
            account: transaction.initiator_account(),
            received_at_ms: transaction.received_timestamp_ms,
            fee_data: transaction.common_data.fee.clone(),
        }
    }
}

/// Mempool score of transaction. Used to prioritize L2 transactions in mempool
/// Currently trivial ordering is used based on received at timestamp
#[derive(Eq, PartialEq, Clone, Debug, Hash)]
pub struct MempoolScore {
    pub account: Address,
    pub received_at_ms: u64,
    // Not used for actual scoring, but state keeper would request
    // transactions that have acceptable fee values (so transactions
    // with fee too low would be ignored until prices go down).
    pub fee_data: Fee,
}

impl MempoolScore {
    /// Checks whether transaction matches requirements provided by state keeper.
    pub fn matches_filter(&self, filter: &L2TxFilter) -> bool {
        self.fee_data.max_fee_per_gas >= U256::from(filter.fee_per_gas)
            && self.fee_data.gas_per_pubdata_limit >= U256::from(filter.gas_per_pubdata)
    }
}

impl Ord for MempoolScore {
    fn cmp(&self, other: &MempoolScore) -> Ordering {
        match self.received_at_ms.cmp(&other.received_at_ms).reverse() {
            Ordering::Equal => {}
            ordering => return ordering,
        }
        self.account.cmp(&other.account)
    }
}

impl PartialOrd for MempoolScore {
    fn partial_cmp(&self, other: &MempoolScore) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Default)]
pub(crate) struct InsertionMetadata {
    pub new_score: Option<MempoolScore>,
    pub previous_score: Option<MempoolScore>,
    pub is_new: bool,
}

/// Structure that can be used by state keeper to describe
/// criteria for transaction it wants to fetch.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct L2TxFilter {
    /// Batch fee model input. It typically includes things like L1 gas price, L2 fair fee, etc.
    pub fee_input: BatchFeeInput,
    /// Effective fee price for the transaction. The price of 1 gas in wei.
    pub fee_per_gas: u64,
    /// Effective pubdata price in gas for transaction. The number of gas per 1 pubdata byte.
    pub gas_per_pubdata: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks the filter logic.
    #[test]
    fn filter() {
        fn filter(fee_per_gas: u64, gas_per_pubdata: u32) -> L2TxFilter {
            L2TxFilter {
                fee_input: BatchFeeInput::sensible_l1_pegged_default(),
                fee_per_gas,
                gas_per_pubdata,
            }
        }

        const MAX_FEE_PER_GAS: u64 = 100u64;
        const MAX_PRIORITY_FEE_PER_GAS: u32 = 100u32;
        const GAS_PER_PUBDATA_LIMIT: u32 = 100u32;

        let score = MempoolScore {
            account: Address::random(),
            received_at_ms: Default::default(), // Not important
            fee_data: Fee {
                gas_limit: Default::default(), // Not important
                max_fee_per_gas: U256::from(MAX_FEE_PER_GAS),
                max_priority_fee_per_gas: U256::from(MAX_PRIORITY_FEE_PER_GAS),
                gas_per_pubdata_limit: U256::from(GAS_PER_PUBDATA_LIMIT),
            },
        };

        let noop_filter = filter(0, 0);
        assert!(
            score.matches_filter(&noop_filter),
            "Noop filter should always match"
        );

        let max_gas_filter = filter(MAX_FEE_PER_GAS, 0);
        assert!(
            score.matches_filter(&max_gas_filter),
            "Correct max gas should be accepted"
        );

        let pubdata_filter = filter(0, GAS_PER_PUBDATA_LIMIT);
        assert!(
            score.matches_filter(&pubdata_filter),
            "Correct pubdata price should be accepted"
        );

        let decline_gas_filter = filter(MAX_FEE_PER_GAS + 1, 0);
        assert!(
            !score.matches_filter(&decline_gas_filter),
            "Incorrect max gas should be rejected"
        );

        let decline_pubdata_filter = filter(0, GAS_PER_PUBDATA_LIMIT + 1);
        assert!(
            !score.matches_filter(&decline_pubdata_filter),
            "Incorrect pubdata price should be rejected"
        );
    }
}
