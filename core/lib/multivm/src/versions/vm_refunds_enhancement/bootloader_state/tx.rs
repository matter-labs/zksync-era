use crate::vm_refunds_enhancement::types::internals::TransactionData;
use zksync_types::{L2ChainId, H256, U256};
use zksync_utils::bytecode::CompressedBytecodeInfo;

/// Information about tx necessary for execution in bootloader.
#[derive(Debug, Clone)]
pub(super) struct BootloaderTx {
    pub(super) hash: H256,
    /// Encoded transaction
    pub(super) encoded: Vec<U256>,
    /// Compressed bytecodes, which has been published during this transaction
    pub(super) compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    /// Refunds for this transaction
    pub(super) refund: u32,
    /// Gas overhead
    pub(super) gas_overhead: u32,
    /// Gas Limit for this transaction. It can be different from the gaslimit inside the transaction
    pub(super) trusted_gas_limit: U256,
    /// Offset of the tx in bootloader memory
    pub(super) offset: usize,
}

impl BootloaderTx {
    pub(super) fn new(
        tx: TransactionData,
        predefined_refund: u32,
        predefined_overhead: u32,
        trusted_gas_limit: U256,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
        offset: usize,
        chain_id: L2ChainId,
    ) -> Self {
        let hash = tx.tx_hash(chain_id);
        Self {
            hash,
            encoded: tx.into_tokens(),
            compressed_bytecodes,
            refund: predefined_refund,
            gas_overhead: predefined_overhead,
            trusted_gas_limit,
            offset,
        }
    }

    pub(super) fn encoded_len(&self) -> usize {
        self.encoded.len()
    }
}
