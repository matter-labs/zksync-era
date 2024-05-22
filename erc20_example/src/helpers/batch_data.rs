use std::fmt::Display;

use colored::Colorize;
use ethers::abi::Hash;
use zksync_types::U64;

use super::L1TxData;

pub struct BatchData {
    /// Batch number
    pub batch_number: U64,
    /// Hashes of the L2 transactions included in the batch
    pub l2_txs_hashes: Vec<Hash>,
    /// Number of L2 transactions included in the batch
    pub l2_txs: u128,
    /// Commit transaction data
    pub commit_tx_data: L1TxData,
    /// Prove transaction data
    pub prove_tx_data: L1TxData,
    /// Execute transaction data
    pub execute_tx_data: L1TxData,
}

impl Display for BatchData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}",
            format!(
                "{formatted_label} (L2 txs included: {formatted_l2_txs})",
                formatted_label = format!(
                    "Batch #{formatted_batch_number}",
                    formatted_batch_number = self.batch_number.as_u64()
                )
                .bright_magenta()
                .bold(),
                formatted_l2_txs = self.l2_txs.to_string().bright_yellow(),
            )
        )?;
        writeln!(f, "{}", self.commit_tx_data)?;
        writeln!(f, "{}", self.prove_tx_data)?;
        writeln!(f, "{}", self.execute_tx_data)?;
        Ok(())
    }
}
