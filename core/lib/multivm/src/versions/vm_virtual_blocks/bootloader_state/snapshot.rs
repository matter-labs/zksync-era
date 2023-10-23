use zksync_types::H256;

#[derive(Debug, Clone)]
pub(crate) struct BootloaderStateSnapshot {
    /// ID of the next transaction to be executed.
    pub(crate) tx_to_execute: usize,
    /// Stored l2 blocks in bootloader memory
    pub(crate) l2_blocks_len: usize,
    /// Snapshot of the last l2 block. Only this block could be changed during the rollback
    pub(crate) last_l2_block: L2BlockSnapshot,
    /// The number of 32-byte words spent on the already included compressed bytecodes.
    pub(crate) compressed_bytecodes_encoding: usize,
    /// Current offset of the free space in the bootloader memory.
    pub(crate) free_tx_offset: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct L2BlockSnapshot {
    /// The rolling hash of all the transactions in the miniblock
    pub(crate) txs_rolling_hash: H256,
    /// The number of transactions in the last l2 block
    pub(crate) txs_len: usize,
}
