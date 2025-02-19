use zksync_types::{Address, L2ChainId, H256, U256};

use crate::{interface::CompressedBytecodeInfo, vm_latest::types::TransactionData};

#[derive(Debug)]
pub(crate) struct EcRecoverCall {
    /// `ecrecover` input obtained by concatenating:
    ///
    /// - 32-byte signed tx hash
    /// - `signature.v` (0 or 1), represented as a big-endian 32-byte word
    /// - `signature.r` (32 bytes)
    /// - `signature.s` (32 bytes)
    pub input: [u8; 128],
    /// Expected call output (= transaction initiator address).
    pub output: Address,
}

/// Information about tx necessary for execution in bootloader.
#[derive(Debug, Clone)]
pub(crate) struct BootloaderTx {
    pub(crate) hash: H256,
    /// Encoded transaction
    pub(crate) encoded: Vec<U256>,
    /// Compressed bytecodes, which has been published during this transaction
    pub(crate) compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    /// Refunds for this transaction
    pub(crate) refund: u64,
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
        predefined_refund: u64,
        predefined_overhead: u32,
        trusted_gas_limit: U256,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
        offset: usize,
        chain_id: L2ChainId,
    ) -> (Self, Option<EcRecoverCall>) {
        let (signed_hash, hash) = tx.signed_and_tx_hashes(chain_id);
        let expected_ecrecover_call = signed_hash.and_then(|signed_hash| {
            let mut input = [0_u8; 128];
            input[..32].copy_from_slice(signed_hash.as_bytes());
            let (v, r_and_s) = tx.parse_signature()?;
            input[63] = v as u8;
            input[64..].copy_from_slice(r_and_s);
            Some(EcRecoverCall {
                input,
                output: tx.from,
            })
        });

        let this = Self {
            hash,
            encoded: tx.into_tokens(),
            compressed_bytecodes,
            refund: predefined_refund,
            gas_overhead: predefined_overhead,
            trusted_gas_limit,
            offset,
        };
        (this, expected_ecrecover_call)
    }

    pub(super) fn encoded_len(&self) -> usize {
        self.encoded.len()
    }
}
