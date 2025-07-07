use std::cmp::Ordering;

use once_cell::sync::OnceCell;
use zksync_types::{vm::VmVersion, L2ChainId, ProtocolVersionId, U256};
use zksync_vm_interface::pubdata::PubdataBuilder;

use super::{
    tx::{BootloaderTx, EcRecoverCall},
    utils::apply_pubdata_to_memory,
};
use crate::{
    interface::{
        pubdata::PubdataInput, BootloaderMemory, CompressedBytecodeInfo, L2BlockEnv,
        TxExecutionMode,
    },
    vm_latest::{
        bootloader::{
            l2_block::BootloaderL2Block,
            snapshot::BootloaderStateSnapshot,
            utils::{apply_l2_block, apply_tx_to_memory, L2BlockApplicationConfig, NewBlockConfig},
        },
        constants::get_tx_description_offset,
        types::TransactionData,
        utils::l2_blocks::assert_next_block,
        MultiVmSubversion,
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
#[derive(Debug)]
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
    /// Information about the the pubdata that will be needed to supply to the L1Messenger
    pubdata_information: OnceCell<PubdataInput>,
    /// The number of applied interop roots
    number_of_applied_interop_roots: usize,
    /// Protocol version.
    protocol_version: ProtocolVersionId,
    /// Protocol subversion
    subversion: MultiVmSubversion,
}

impl BootloaderState {
    pub(crate) fn new(
        execution_mode: TxExecutionMode,
        initial_memory: BootloaderMemory,
        first_l2_block: L2BlockEnv,
        protocol_version: ProtocolVersionId,
    ) -> Self {
        let l2_block = BootloaderL2Block::new(first_l2_block, 0);
        Self {
            tx_to_execute: 0,
            compressed_bytecodes_encoding: 0,
            l2_blocks: vec![l2_block],
            initial_memory,
            execution_mode,
            free_tx_offset: 0,
            pubdata_information: Default::default(),
            number_of_applied_interop_roots: 0,
            protocol_version,
            subversion: MultiVmSubversion::try_from(VmVersion::from(protocol_version)).unwrap(),
        }
    }

    pub(crate) fn set_refund_for_current_tx(&mut self, refund: u64) {
        let current_tx = self.current_tx();
        // We can't set the refund for the latest tx or using the latest l2_block for fining tx
        // Because we can fill the whole batch first and then execute txs one by one
        let tx = self.find_tx_mut(current_tx);
        tx.refund = refund;
    }

    pub(crate) fn set_pubdata_input(&mut self, info: PubdataInput) {
        self.pubdata_information
            .set(info)
            .expect("Pubdata information is already set");
    }

    pub(crate) fn start_new_l2_block(&mut self, l2_block: L2BlockEnv) {
        let last_block = self.last_l2_block();
        assert!(
            !last_block.txs.is_empty(),
            "Can not create new miniblocks on top of empty ones"
        );
        assert_next_block(&last_block.l2_block(), &l2_block, self.protocol_version);
        self.push_l2_block(l2_block);
    }

    /// This method bypass sanity checks and should be used carefully.
    pub(crate) fn push_l2_block(&mut self, l2_block: L2BlockEnv) {
        self.l2_blocks
            .push(BootloaderL2Block::new(l2_block, self.free_tx_index()))
    }

    pub(crate) fn get_vm_subversion(&self) -> MultiVmSubversion {
        self.subversion
    }

    pub(crate) fn get_preexisting_blocks_number(&self) -> usize {
        self.l2_blocks.len().saturating_sub(1)
    }

    pub(crate) fn get_new_block_config(&self) -> NewBlockConfig {
        NewBlockConfig {
            number_of_applied_interop_roots: self.number_of_applied_interop_roots,
            preexisting_blocks_number: self.get_preexisting_blocks_number(),
        }
    }

    pub(crate) fn push_tx(
        &mut self,
        tx: TransactionData,
        predefined_overhead: u32,
        predefined_refund: u64,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
        trusted_ergs_limit: U256,
        chain_id: L2ChainId,
    ) -> (BootloaderMemory, Option<EcRecoverCall>) {
        let tx_offset = self.free_tx_offset();
        let (bootloader_tx, ecrecover_call) = BootloaderTx::new(
            tx,
            predefined_refund,
            predefined_overhead,
            trusted_ergs_limit,
            compressed_bytecodes,
            tx_offset,
            chain_id,
        );

        let mut memory = vec![];
        let new_block_config = if self.last_l2_block().txs.is_empty() {
            Some(self.get_new_block_config())
        } else {
            None
        };
        let config: L2BlockApplicationConfig = L2BlockApplicationConfig {
            tx_index: self.free_tx_index(),
            subversion: self.subversion,
            new_block_config,
        };
        let (compressed_bytecode_size, number_of_applied_interop_roots) = apply_tx_to_memory(
            &mut memory,
            &bootloader_tx,
            self.last_l2_block(),
            self.free_tx_offset(),
            self.compressed_bytecodes_encoding,
            self.execution_mode,
            config,
        );
        self.compressed_bytecodes_encoding += compressed_bytecode_size;
        self.free_tx_offset = tx_offset + bootloader_tx.encoded_len();
        self.number_of_applied_interop_roots += number_of_applied_interop_roots;
        self.last_mut_l2_block().push_tx(bootloader_tx);
        (memory, ecrecover_call)
    }

    pub(crate) fn last_l2_block(&self) -> &BootloaderL2Block {
        self.l2_blocks.last().unwrap()
    }

    pub(crate) fn get_pubdata_information(&self) -> &PubdataInput {
        self.pubdata_information
            .get()
            .expect("Pubdata information is not set")
    }

    pub(crate) fn settlement_layer_pubdata(&self, pubdata_builder: &dyn PubdataBuilder) -> Vec<u8> {
        let pubdata_information = self
            .pubdata_information
            .get()
            .expect("Pubdata information is not set");

        pubdata_builder.settlement_layer_pubdata(pubdata_information, self.protocol_version)
    }

    fn last_mut_l2_block(&mut self) -> &mut BootloaderL2Block {
        self.l2_blocks.last_mut().unwrap()
    }

    /// Apply all bootloader transaction to the initial memory
    pub(crate) fn bootloader_memory(
        &self,
        pubdata_builder: &dyn PubdataBuilder,
    ) -> BootloaderMemory {
        let mut initial_memory = self.initial_memory.clone();
        let mut offset = 0;
        let mut compressed_bytecodes_offset = 0;
        let mut tx_index = 0;
        let mut applied_interop_roots_offset = 0;
        for (i, l2_block) in self.l2_blocks.iter().enumerate() {
            for (num, tx) in l2_block.txs.iter().enumerate() {
                let new_block_config = if num == 0 {
                    Some(NewBlockConfig {
                        number_of_applied_interop_roots: applied_interop_roots_offset,
                        preexisting_blocks_number: i,
                    })
                } else {
                    None
                };
                let config: L2BlockApplicationConfig = L2BlockApplicationConfig {
                    tx_index,
                    subversion: self.subversion,
                    new_block_config,
                };
                let (compressed_bytecodes_size, number_of_applied_interop_roots) =
                    apply_tx_to_memory(
                        &mut initial_memory,
                        tx,
                        l2_block,
                        offset,
                        compressed_bytecodes_offset,
                        self.execution_mode,
                        config,
                    );
                offset += tx.encoded_len();
                compressed_bytecodes_offset += compressed_bytecodes_size;
                tx_index += 1;
                applied_interop_roots_offset += number_of_applied_interop_roots;
            }
            if l2_block.txs.is_empty() {
                let new_block_config = Some(NewBlockConfig {
                    number_of_applied_interop_roots: applied_interop_roots_offset,
                    preexisting_blocks_number: self.get_preexisting_blocks_number(),
                });
                applied_interop_roots_offset += apply_l2_block(
                    &mut initial_memory,
                    l2_block,
                    tx_index,
                    self.subversion,
                    new_block_config,
                );
            }
        }

        let pubdata_information = self
            .pubdata_information
            .get()
            .expect("Empty pubdata information");

        apply_pubdata_to_memory(
            &mut initial_memory,
            pubdata_builder,
            pubdata_information,
            self.protocol_version,
            self.subversion,
        );
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
        get_tx_description_offset(self.subversion) + self.find_tx(tx_index).offset
    }

    fn find_tx(&self, tx_index: usize) -> &BootloaderTx {
        for block in self.l2_blocks.iter().rev() {
            if tx_index >= block.first_tx_index {
                return &block.txs[tx_index - block.first_tx_index];
            }
        }
        panic!("The tx with index {} must exist", tx_index)
    }

    fn find_tx_mut(&mut self, tx_index: usize) -> &mut BootloaderTx {
        for block in self.l2_blocks.iter_mut().rev() {
            if tx_index >= block.first_tx_index {
                return &mut block.txs[tx_index - block.first_tx_index];
            }
        }
        panic!("The tx with index {} must exist", tx_index)
    }

    pub(crate) fn get_snapshot(&self) -> BootloaderStateSnapshot {
        BootloaderStateSnapshot {
            tx_to_execute: self.tx_to_execute,
            l2_blocks_len: self.l2_blocks.len(),
            last_l2_block: self.last_l2_block().make_snapshot(),
            compressed_bytecodes_encoding: self.compressed_bytecodes_encoding,
            free_tx_offset: self.free_tx_offset,
            is_pubdata_information_provided: self.pubdata_information.get().is_some(),
            number_of_applied_interop_roots: self.number_of_applied_interop_roots,
        }
    }

    pub(crate) fn apply_snapshot(&mut self, snapshot: BootloaderStateSnapshot) {
        self.tx_to_execute = snapshot.tx_to_execute;
        self.compressed_bytecodes_encoding = snapshot.compressed_bytecodes_encoding;
        self.free_tx_offset = snapshot.free_tx_offset;
        self.number_of_applied_interop_roots = snapshot.number_of_applied_interop_roots;
        match self.l2_blocks.len().cmp(&snapshot.l2_blocks_len) {
            Ordering::Greater => self.l2_blocks.truncate(snapshot.l2_blocks_len),
            Ordering::Less => panic!("Applying snapshot from future is not supported"),
            Ordering::Equal => {}
        }
        self.last_mut_l2_block()
            .apply_snapshot(snapshot.last_l2_block);

        if !snapshot.is_pubdata_information_provided {
            self.pubdata_information = Default::default();
        } else {
            // Under the correct usage of the snapshots of the bootloader state,
            // this assertion should never fail, i.e. since the pubdata information
            // can be set only once. However, we have this assertion just in case.
            assert!(
                self.pubdata_information.get().is_some(),
                "Snapshot with no pubdata can not rollback to snapshot with one"
            );
        }
    }

    pub(crate) fn protocol_version(&self) -> ProtocolVersionId {
        self.protocol_version
    }
}
