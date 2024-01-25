use zksync_types::Transaction;

use crate::vm_latest::types::internals::TransactionData;

/// Extension  for transactions, specific for VM. Required for bypassing the orphan rule
pub trait TransactionVmExt {
    /// Get the size of the transaction in tokens.
    fn bootloader_encoding_size(&self) -> usize;
}

impl TransactionVmExt for Transaction {
    fn bootloader_encoding_size(&self) -> usize {
        let transaction_data: TransactionData = self.clone().into();
        transaction_data.into_tokens().len()
    }
}
