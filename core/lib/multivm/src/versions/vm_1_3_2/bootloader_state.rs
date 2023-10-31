use crate::vm_1_3_2::vm_with_bootloader::TX_DESCRIPTION_OFFSET;

/// Intermediate bootloader-related VM state.
///
/// Required to process transactions one by one (since we intercept the VM execution to execute
/// transactions and add new ones to the memory on the fly).
/// Think about it like a two-pointer scheme: one pointer (`free_tx_index`) tracks the end of the
/// initialized memory; while another (`tx_to_execute`) tracks our progess in this initialized memory.
/// This is required since it's possible to push several transactions to the bootloader memory and then
/// execute it one by one.
///
/// Serves two purposes:
/// - Tracks where next tx should be pushed to in the bootloader memory.
/// - Tracks which transaction should be executed next.
#[derive(Debug, Default, Clone)]
pub(crate) struct BootloaderState {
    /// Memory offset (in words) for the next transaction data.
    free_tx_offset: usize,
    /// ID of the next transaction to be executed.
    /// See the structure doc-comment for a better explanation of purpose.
    tx_to_execute: usize,
    /// Vector that contains sizes of all pushed transactions.
    tx_sizes: Vec<usize>,

    /// The number of 32-byte words spent on the already included compressed bytecodes.
    compressed_bytecodes_encoding: usize,
}

impl BootloaderState {
    /// Creates an empty bootloader state.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Notifies the state about the fact that new transaction was pushed into the memory.
    pub(crate) fn add_tx_data(&mut self, tx_size: usize) {
        self.free_tx_offset += tx_size;
        self.tx_sizes.push(tx_size);
    }

    /// Returns the next "free" transaction index.
    pub(crate) fn free_tx_index(&self) -> usize {
        self.tx_sizes.len()
    }

    /// Returns the next index of transaction to execute.
    pub(crate) fn tx_to_execute(&self) -> usize {
        self.tx_to_execute
    }

    /// Returns the memory offset for the new transaction.
    pub(crate) fn free_tx_offset(&self) -> usize {
        self.free_tx_offset
    }

    /// Returns the ID of the next transaction to be executed and increments the local transaction counter.
    pub(crate) fn next_unexecuted_tx(&mut self) -> usize {
        assert!(
            self.tx_to_execute < self.tx_sizes.len(),
            "Attempt to execute tx that was not pushed to memory. Tx ID: {}, txs in bootloader: {}",
            self.tx_to_execute,
            self.tx_sizes.len()
        );

        let old = self.tx_to_execute;
        self.tx_to_execute += 1;
        old
    }

    /// Returns the size of the transaction with given index.
    /// Panics if there is no such transaction.
    /// Use it after TODO (SMA-1715): make users pay for the overhead
    #[allow(dead_code)]
    pub(crate) fn get_tx_size(&self, tx_index: usize) -> usize {
        self.tx_sizes[tx_index]
    }

    pub(crate) fn get_tx_description_offset(&self, tx_index: usize) -> usize {
        TX_DESCRIPTION_OFFSET + self.tx_sizes.iter().take(tx_index).sum::<usize>()
    }

    pub(crate) fn add_compressed_bytecode(&mut self, bytecode_compression_encoding_length: usize) {
        self.compressed_bytecodes_encoding += bytecode_compression_encoding_length;
    }

    pub(crate) fn get_compressed_bytecodes(&self) -> usize {
        self.compressed_bytecodes_encoding
    }
}

#[cfg(test)]
mod tests {
    use super::BootloaderState;

    #[test]
    fn workflow() {
        let mut state = BootloaderState::new();
        assert_eq!(state.free_tx_index(), 0);
        assert_eq!(state.free_tx_offset(), 0);

        state.add_tx_data(2);
        assert_eq!(state.free_tx_index(), 1);
        assert_eq!(state.free_tx_offset(), 2);

        state.add_tx_data(4);
        assert_eq!(state.free_tx_index(), 2);
        assert_eq!(state.free_tx_offset(), 6);

        assert_eq!(state.next_unexecuted_tx(), 0);
        assert_eq!(state.next_unexecuted_tx(), 1);
    }

    #[test]
    #[should_panic(
        expected = "Attempt to execute tx that was not pushed to memory. Tx ID: 0, txs in bootloader: 0"
    )]
    fn get_not_pushed_tx() {
        let mut state = BootloaderState::new();
        state.next_unexecuted_tx();
    }
}
