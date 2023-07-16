use std::str::FromStr;

use sqlx::types::chrono::{DateTime, NaiveDateTime, Utc};

use zksync_contracts::BaseSystemContractsHashes;
use zksync_types::api::en::SyncBlock;
use zksync_types::Transaction;
use zksync_types::{Address, L1BatchNumber, MiniblockNumber, H256};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageSyncBlock {
    pub number: i64,
    pub l1_batch_number: i64,
    pub last_batch_miniblock: Option<i64>,
    pub timestamp: i64,
    pub root_hash: Option<Vec<u8>>,
    pub commit_tx_hash: Option<String>,
    pub committed_at: Option<NaiveDateTime>,
    pub prove_tx_hash: Option<String>,
    pub proven_at: Option<NaiveDateTime>,
    pub execute_tx_hash: Option<String>,
    pub executed_at: Option<NaiveDateTime>,
    // L1 gas price assumed in the corresponding batch
    pub l1_gas_price: i64,
    // L2 gas price assumed in the corresponding batch
    pub l2_fair_gas_price: i64,
    pub bootloader_code_hash: Option<Vec<u8>>,
    pub default_aa_code_hash: Option<Vec<u8>>,
    pub fee_account_address: Option<Vec<u8>>, // May be None if the block is not yet sealed
}

impl StorageSyncBlock {
    pub(crate) fn into_sync_block(
        self,
        current_operator_address: Address,
        transactions: Option<Vec<Transaction>>,
    ) -> SyncBlock {
        SyncBlock {
            number: MiniblockNumber(self.number as u32),
            l1_batch_number: L1BatchNumber(self.l1_batch_number as u32),
            last_in_batch: self
                .last_batch_miniblock
                .map(|n| n == self.number)
                .unwrap_or(false),
            timestamp: self.timestamp as u64,
            root_hash: self.root_hash.as_deref().map(H256::from_slice),
            commit_tx_hash: self
                .commit_tx_hash
                .as_deref()
                .map(|hash| H256::from_str(hash).expect("Incorrect commit_tx hash")),
            committed_at: self
                .committed_at
                .map(|committed_at| DateTime::<Utc>::from_utc(committed_at, Utc)),
            prove_tx_hash: self
                .prove_tx_hash
                .as_deref()
                .map(|hash| H256::from_str(hash).expect("Incorrect prove_tx hash")),
            proven_at: self
                .proven_at
                .map(|proven_at| DateTime::<Utc>::from_utc(proven_at, Utc)),
            execute_tx_hash: self
                .execute_tx_hash
                .as_deref()
                .map(|hash| H256::from_str(hash).expect("Incorrect execute_tx hash")),
            executed_at: self
                .executed_at
                .map(|executed_at| DateTime::<Utc>::from_utc(executed_at, Utc)),
            l1_gas_price: self.l1_gas_price as u64,
            l2_fair_gas_price: self.l2_fair_gas_price as u64,
            base_system_contracts_hashes: BaseSystemContractsHashes {
                bootloader: self
                    .bootloader_code_hash
                    .map(|bootloader_code_hash| H256::from_slice(&bootloader_code_hash))
                    .expect("Should not be none"),
                default_aa: self
                    .default_aa_code_hash
                    .map(|default_aa_code_hash| H256::from_slice(&default_aa_code_hash))
                    .expect("Should not be none"),
            },
            operator_address: self
                .fee_account_address
                .map(|fee_account_address| Address::from_slice(&fee_account_address))
                .unwrap_or(current_operator_address),
            transactions,
        }
    }
}
