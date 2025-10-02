use zksync_types::Transaction;

use crate::vm_latest::types::TransactionData;

/// Extension  for transactions, specific for VM. Required for bypassing the orphan rule
pub trait TransactionVmExt {
    /// Get the size of the transaction in tokens.
    fn bootloader_encoding_size(&self) -> usize;
}

impl TransactionVmExt for Transaction {
    fn bootloader_encoding_size(&self) -> usize {
        // Since we want to just measure the encoding size, `use_evm_emulator` arg doesn't matter here,
        // so we use a more lenient option.
        let transaction_data = TransactionData::new(self.clone(), true);
        transaction_data.into_tokens().len()
    }
}
