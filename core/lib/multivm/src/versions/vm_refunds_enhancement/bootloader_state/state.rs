use std::cmp::Ordering;

use zksync_types::{L2ChainId, U256};

use super::tx::BootloaderTx;
use crate::{
    interface::{BootloaderMemory, CompressedBytecodeInfo, L2BlockEnv, TxExecutionMode},
    vm_refunds_enhancement::{
        bootloader_state::{
            l2_block::BootloaderL2Block,
            snapshot::BootloaderStateSnapshot,
            utils::{apply_l2_block, apply_tx_to_memory},
        },
        constants::TX_DESCRIPTION_OFFSET,
        types::internals::TransactionData,
        utils::l2_blocks::assert_next_block,
    },
};
/// Intermediate bootloader-related VM state.
///
/// Required to process transactions one by one (since we intercept the VM execution to execute
/// transactions and add new ones to the memory on the fly).
/// Keeps tracking everything related to the bootloader memory and can restore the whole memory.
///
///
/// Serves two purposes:
/// - Tracks where next tx should be pushed to in the bootloader memory.
/// - Tracks which transaction should be executed next.
#[derive(Debug, Clone)]
pub struct BootloaderState {
    /// ID of the next transaction to be executed.
    /// See the structure doc-comment for a better explanation of purpose.
    tx_to_execute: usize,
    /// Stored txs in bootloader memory
    l2_blocks: Vec<BootloaderL2Block>,
    /// The number of 32-byte words spent on the already included compressed bytecodes.
    compressed_bytecodes_encoding: usize,
    /// Initial memory of bootloader
    initial_memory: BootloaderMemory,
    /// Mode of txs for execution, it can be changed once per vm lunch
    execution_mode: TxExecutionMode,
    /// Current offset of the free space in the bootloader memory.
    free_tx_offset: usize,
}

impl BootloaderState {
    pub(crate) fn new(
        execution_mode: TxExecutionMode,
        initial_memory: BootloaderMemory,
        first_l2_block: L2BlockEnv,
    ) -> Self {
        let l2_block = BootloaderL2Block::new(first_l2_block, 0);
        Self {
            tx_to_execute: 0,
            compressed_bytecodes_encoding: 0,
            l2_blocks: vec![l2_block],
            initial_memory,
            execution_mode,
            free_tx_offset: 0,
        }
    }

    pub(crate) fn set_refund_for_current_tx(&mut self, refund: u32) {
        let current_tx = self.current_tx();
        // We can't set the refund for the latest tx or using the latest l2_block for fining tx
        // Because we can fill the whole batch first and then execute txs one by one
        let tx = self.find_tx_mut(current_tx);
        tx.refund = refund;
    }

    pub(crate) fn start_new_l2_block(&mut self, l2_block: L2BlockEnv) {
        let last_block = self.last_l2_block();
        assert!(
            !last_block.txs.is_empty(),
            "Can not create new miniblocks on top of empty ones"
        );
        assert_next_block(&last_block.l2_block(), &l2_block);
        self.push_l2_block(l2_block);
    }

    /// This method bypass sanity checks and should be used carefully.
    pub(crate) fn push_l2_block(&mut self, l2_block: L2BlockEnv) {
        self.l2_blocks
            .push(BootloaderL2Block::new(l2_block, self.free_tx_index()))
    }

    pub(crate) fn push_tx(
        &mut self,
        tx: TransactionData,
        predefined_overhead: u32,
        predefined_refund: u32,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
        trusted_ergs_limit: U256,
        chain_id: L2ChainId,
    ) -> BootloaderMemory {
        let tx_offset = self.free_tx_offset();
        let bootloader_tx = BootloaderTx::new(
            tx,
            predefined_refund,
            predefined_overhead,
            trusted_ergs_limit,
            compressed_bytecodes,
            tx_offset,
            chain_id,
        );

        let mut memory = vec![];
        let compressed_bytecode_size = apply_tx_to_memory(
            &mut memory,
            &bootloader_tx,
            self.last_l2_block(),
            self.free_tx_index(),
            self.free_tx_offset(),
            self.compressed_bytecodes_encoding,
            self.execution_mode,
            self.last_l2_block().txs.is_empty(),
        );
        self.compressed_bytecodes_encoding += compressed_bytecode_size;
        self.free_tx_offset = tx_offset + bootloader_tx.encoded_len();
        self.last_mut_l2_block().push_tx(bootloader_tx);
        memory
    }

    pub(crate) fn last_l2_block(&self) -> &BootloaderL2Block {
        self.l2_blocks.last().unwrap()
    }

    fn last_mut_l2_block(&mut self) -> &mut BootloaderL2Block {
        self.l2_blocks.last_mut().unwrap()
    }

    /// Apply all bootloader transaction to the initial memory
    pub(crate) fn bootloader_memory(&self) -> BootloaderMemory {
        let mut initial_memory = self.initial_memory.clone();
        let mut offset = 0;
        let mut compressed_bytecodes_offset = 0;
        let mut tx_index = 0;
        for l2_block in &self.l2_blocks {
            for (num, tx) in l2_block.txs.iter().enumerate() {
                let compressed_bytecodes_size = apply_tx_to_memory(
                    &mut initial_memory,
                    tx,
                    l2_block,
                    tx_index,
                    offset,
                    compressed_bytecodes_offset,
                    self.execution_mode,
                    num == 0,
                );
                offset += tx.encoded_len();
                compressed_bytecodes_offset += compressed_bytecodes_size;
                tx_index += 1;
            }
            if l2_block.txs.is_empty() {
                apply_l2_block(&mut initial_memory, l2_block, tx_index)
            }
        }
        initial_memory
    }

    fn free_tx_offset(&self) -> usize {
        self.free_tx_offset
    }

    pub(crate) fn free_tx_index(&self) -> usize {
        let l2_block = self.last_l2_block();
        l2_block.first_tx_index + l2_block.txs.len()
    }

    pub(crate) fn get_last_tx_compressed_bytecodes(&self) -> &[CompressedBytecodeInfo] {
        if let Some(tx) = self.last_l2_block().txs.last() {
            &tx.compressed_bytecodes
        } else {
            &[]
        }
    }

    /// Returns the id of current tx
    pub(crate) fn current_tx(&self) -> usize {
        self.tx_to_execute
            .checked_sub(1)
            .expect("There are no current tx to execute")
    }

    /// Returns the ID of the next transaction to be executed and increments the local transaction counter.
    pub(crate) fn move_tx_to_execute_pointer(&mut self) -> usize {
        assert!(
            self.tx_to_execute < self.free_tx_index(),
            "Attempt to execute tx that was not pushed to memory. Tx ID: {}, txs in bootloader: {}",
            self.tx_to_execute,
            self.free_tx_index()
        );

        let old = self.tx_to_execute;
        self.tx_to_execute += 1;
        old
    }

    /// Get offset of tx description
    pub(crate) fn get_tx_description_offset(&self, tx_index: usize) -> usize {
        TX_DESCRIPTION_OFFSET + self.find_tx(tx_index).offset
    }

    pub(crate) fn insert_fictive_l2_block(&mut self) -> &BootloaderL2Block {
        let block = self.last_l2_block();
        if !block.txs.is_empty() {
            self.start_new_l2_block(L2BlockEnv {
                timestamp: block.timestamp + 1,
                number: block.number + 1,
                prev_block_hash: block.get_hash(),
                max_virtual_blocks_to_create: 1,
                interop_roots: vec![],
            });
        }
        self.last_l2_block()
    }

    fn find_tx(&self, tx_index: usize) -> &BootloaderTx {
        for block in self.l2_blocks.iter().rev() {
            if tx_index >= block.first_tx_index {
                return &block.txs[tx_index - block.first_tx_index];
            }
        }
        panic!("The tx with this index must exist")
    }

    fn find_tx_mut(&mut self, tx_index: usize) -> &mut BootloaderTx {
        for block in self.l2_blocks.iter_mut().rev() {
            if tx_index >= block.first_tx_index {
                return &mut block.txs[tx_index - block.first_tx_index];
            }
        }
        panic!("The tx with this index must exist")
    }

    pub(crate) fn get_snapshot(&self) -> BootloaderStateSnapshot {
        BootloaderStateSnapshot {
            tx_to_execute: self.tx_to_execute,
            l2_blocks_len: self.l2_blocks.len(),
            last_l2_block: self.last_l2_block().make_snapshot(),
            compressed_bytecodes_encoding: self.compressed_bytecodes_encoding,
            free_tx_offset: self.free_tx_offset,
        }
    }

    pub(crate) fn apply_snapshot(&mut self, snapshot: BootloaderStateSnapshot) {
        self.tx_to_execute = snapshot.tx_to_execute;
        self.compressed_bytecodes_encoding = snapshot.compressed_bytecodes_encoding;
        self.free_tx_offset = snapshot.free_tx_offset;
        match self.l2_blocks.len().cmp(&snapshot.l2_blocks_len) {
            Ordering::Greater => self.l2_blocks.truncate(snapshot.l2_blocks_len),
            Ordering::Less => panic!("Applying snapshot from future is not supported"),
            Ordering::Equal => {}
        }
        self.last_mut_l2_block()
            .apply_snapshot(snapshot.last_l2_block);
    }
}
