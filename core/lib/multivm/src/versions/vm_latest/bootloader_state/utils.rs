use zksync_types::{
    commitment::{L1BatchCommitmentMode, PubdataParams},
    ethabi, U256,
};
use zksync_utils::{bytecode::CompressedBytecodeInfo, bytes_to_be_words, h256_to_u256};

use super::tx::BootloaderTx;
use crate::{
    interface::{BootloaderMemory, CompressedBytecodeInfo, TxExecutionMode},
    utils::bytecode,
    vm_latest::{
        bootloader_state::l2_block::BootloaderL2Block,
        constants::{
            BOOTLOADER_TX_DESCRIPTION_OFFSET, BOOTLOADER_TX_DESCRIPTION_SIZE,
            COMPRESSED_BYTECODES_OFFSET, OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_OFFSET,
            OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS, OPERATOR_REFUNDS_OFFSET,
            TX_DESCRIPTION_OFFSET, TX_OPERATOR_L2_BLOCK_INFO_OFFSET,
            TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO, TX_OVERHEAD_OFFSET, TX_TRUSTED_GAS_LIMIT_OFFSET,
        },
        types::internals::{
            pubdata::{PubdataBuilder, RollupPubdataBuilder, ValidiumPubdataBuilder},
            PubdataInput,
        },
    },
};

pub(super) fn get_memory_for_compressed_bytecodes(
    compressed_bytecodes: &[CompressedBytecodeInfo],
) -> Vec<U256> {
    let memory_addition: Vec<_> = compressed_bytecodes
        .iter()
        .flat_map(bytecode::encode_call)
        .collect();

    bytes_to_be_words(memory_addition)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_tx_to_memory(
    memory: &mut BootloaderMemory,
    bootloader_tx: &BootloaderTx,
    bootloader_l2_block: &BootloaderL2Block,
    tx_index: usize,
    tx_offset: usize,
    compressed_bytecodes_size: usize,
    execution_mode: TxExecutionMode,
    start_new_l2_block: bool,
) -> usize {
    let bootloader_description_offset =
        BOOTLOADER_TX_DESCRIPTION_OFFSET + BOOTLOADER_TX_DESCRIPTION_SIZE * tx_index;
    let tx_description_offset = TX_DESCRIPTION_OFFSET + tx_offset;

    memory.push((
        bootloader_description_offset,
        assemble_tx_meta(execution_mode, true),
    ));

    memory.push((
        bootloader_description_offset + 1,
        U256::from_big_endian(&(32 * tx_description_offset).to_be_bytes()),
    ));

    let refund_offset = OPERATOR_REFUNDS_OFFSET + tx_index;
    memory.push((refund_offset, bootloader_tx.refund.into()));

    let overhead_offset = TX_OVERHEAD_OFFSET + tx_index;
    memory.push((overhead_offset, bootloader_tx.gas_overhead.into()));

    let trusted_gas_limit_offset = TX_TRUSTED_GAS_LIMIT_OFFSET + tx_index;
    memory.push((trusted_gas_limit_offset, bootloader_tx.trusted_gas_limit));

    memory.extend(
        (tx_description_offset..tx_description_offset + bootloader_tx.encoded_len())
            .zip(bootloader_tx.encoded.clone()),
    );

    apply_l2_block_inner(memory, bootloader_l2_block, tx_index, start_new_l2_block);

    // Note, +1 is moving for pointer
    let compressed_bytecodes_offset = COMPRESSED_BYTECODES_OFFSET + 1 + compressed_bytecodes_size;

    let encoded_compressed_bytecodes =
        get_memory_for_compressed_bytecodes(&bootloader_tx.compressed_bytecodes);
    let compressed_bytecodes_encoding = encoded_compressed_bytecodes.len();

    memory.extend(
        (compressed_bytecodes_offset
            ..compressed_bytecodes_offset + encoded_compressed_bytecodes.len())
            .zip(encoded_compressed_bytecodes),
    );
    compressed_bytecodes_encoding
}

pub(crate) fn apply_l2_block(
    memory: &mut BootloaderMemory,
    bootloader_l2_block: &BootloaderL2Block,
    txs_index: usize,
) {
    apply_l2_block_inner(memory, bootloader_l2_block, txs_index, true)
}

fn apply_l2_block_inner(
    memory: &mut BootloaderMemory,
    bootloader_l2_block: &BootloaderL2Block,
    txs_index: usize,
    start_new_l2_block: bool,
) {
    // Since L2 block information start from the `TX_OPERATOR_L2_BLOCK_INFO_OFFSET` and each
    // L2 block info takes `TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO` slots, the position where the L2 block info
    // for this transaction needs to be written is:

    let block_position =
        TX_OPERATOR_L2_BLOCK_INFO_OFFSET + txs_index * TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO;

    memory.extend(vec![
        (block_position, bootloader_l2_block.number.into()),
        (block_position + 1, bootloader_l2_block.timestamp.into()),
        (
            block_position + 2,
            h256_to_u256(bootloader_l2_block.prev_block_hash),
        ),
        (
            block_position + 3,
            if start_new_l2_block {
                bootloader_l2_block.max_virtual_blocks_to_create.into()
            } else {
                U256::zero()
            },
        ),
    ])
}

pub(crate) fn get_encoded_pubdata(
    pubdata_information: PubdataInput,
    pubdata_params: PubdataParams,
    l2_version: bool,
) -> Vec<u8> {
    let pubdata_bytes: Vec<u8> = if pubdata_params.pubdata_type == L1BatchCommitmentMode::Rollup {
        RollupPubdataBuilder::new().build_pubdata(pubdata_information, l2_version)
    } else {
        ValidiumPubdataBuilder::new().build_pubdata(pubdata_information, l2_version)
    };

    if l2_version {
        ethabi::encode(&[
            ethabi::Token::Address(pubdata_params.l2_da_validator_address),
            ethabi::Token::Bytes(pubdata_bytes),
        ])
        .to_vec()
    } else {
        pubdata_bytes
    }
}

pub(crate) fn apply_pubdata_to_memory(
    memory: &mut BootloaderMemory,
    pubdata_information: PubdataInput,
    pubdata_params: PubdataParams,
) {
    // Skipping two slots as they will be filled by the bootloader itself:
    // - One slot is for the selector of the call to the L1Messenger.
    // - The other slot is for the 0x20 offset for the calldata.
    let l1_messenger_pubdata_start_slot = OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_OFFSET + 1;

    let pubdata = get_encoded_pubdata(pubdata_information, pubdata_params, true);

    assert!(
        // Note that unlike the previous version, the difference is `1`, since now it also includes the offset
        pubdata.len() / 32 < OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS,
        "The encoded pubdata is too big"
    );

    pubdata
        .chunks(32)
        .enumerate()
        .for_each(|(slot_offset, value)| {
            memory.push((
                l1_messenger_pubdata_start_slot + slot_offset,
                U256::from(value),
            ))
        });
}

/// Forms a word that contains meta information for the transaction execution.
///
/// # Current layout
///
/// - 0 byte (MSB): server-side tx execution mode
///     In the server, we may want to execute different parts of the transaction in the different context
///     For example, when checking validity, we don't want to actually execute transaction and have side effects.
///
///     Possible values:
///     - 0x00: validate & execute (normal mode)
///     - 0x02: execute but DO NOT validate
///
/// - 31 byte (LSB): whether to execute transaction or not (at all).
pub(super) fn assemble_tx_meta(execution_mode: TxExecutionMode, execute_tx: bool) -> U256 {
    let mut output = [0u8; 32];

    // Set 0 byte (execution mode)
    output[0] = match execution_mode {
        TxExecutionMode::VerifyExecute => 0x00,
        TxExecutionMode::EstimateFee { .. } => 0x00,
        TxExecutionMode::EthCall { .. } => 0x02,
    };

    // Set 31 byte (marker for tx execution)
    output[31] = u8::from(execute_tx);

    U256::from_big_endian(&output)
}
