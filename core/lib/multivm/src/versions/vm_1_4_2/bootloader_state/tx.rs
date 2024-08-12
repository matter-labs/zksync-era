use zksync_types::{L2ChainId, H256, U256};

use crate::{interface::CompressedBytecodeInfo, vm_1_4_2::types::internals::TransactionData};

/// Information about tx necessary for execution in bootloader.
#[derive(Debug, Clone)]
pub(crate) struct BootloaderTx {
    pub(crate) hash: H256,
    /// Encoded transaction
    pub(crate) encoded: Vec<U256>,
    /// Compressed bytecodes, which has been published during this transaction
    pub(crate) compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    /// Refunds for this transaction
    pub(crate) refund: u32,
    /// Gas overhead
    pub(crate) gas_overhead: u32,
    /// Gas Limit for this transaction. It can be different from the gas limit inside the transaction
    pub(crate) trusted_gas_limit: U256,
    /// Offset of the tx in bootloader memory
    pub(crate) offset: usize,
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
