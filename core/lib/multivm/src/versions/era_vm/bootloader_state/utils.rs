use zksync_types::{ethabi, U256};
use zksync_utils::{bytecode::CompressedBytecodeInfo, bytes_to_be_words, h256_to_u256};
use crate::{interface::{BootloaderMemory, TxExecutionMode}, vm_latest::constants::{BOOTLOADER_TX_DESCRIPTION_OFFSET, BOOTLOADER_TX_DESCRIPTION_SIZE, COMPRESSED_BYTECODES_OFFSET, OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_OFFSET, OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS, OPERATOR_REFUNDS_OFFSET, TX_DESCRIPTION_OFFSET, TX_OPERATOR_L2_BLOCK_INFO_OFFSET, TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO, TX_OVERHEAD_OFFSET, TX_TRUSTED_GAS_LIMIT_OFFSET}};

use super::{l2_block::BootloaderL2Block, tx::BootloaderTx};

pub(super) fn get_memory_for_compressed_bytecodes(
    compressed_bytecodes: &[CompressedBytecodeInfo],
) -> Vec<U256> {
    let memory_addition: Vec<_> = compressed_bytecodes
        .iter()
        .flat_map(|x| x.encode_call())
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

    let bootloader_l2_block = if start_new_l2_block {
        bootloader_l2_block.clone()
    } else {
        bootloader_l2_block.interim_version()
    };
    apply_l2_block(memory, &bootloader_l2_block, tx_index);

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
            bootloader_l2_block.max_virtual_blocks_to_create.into(),
        ),
    ])
}

pub(crate) fn apply_pubdata_to_memory(
    memory: &mut BootloaderMemory,
    pubdata_information: PubdataInput,
) {
    // Skipping two slots as they will be filled by the bootloader itself:
    // - One slot is for the selector of the call to the L1Messenger.
    // - The other slot is for the 0x20 offset for the calldata.
    let l1_messenger_pubdata_start_slot = OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_OFFSET + 2;

    // Need to skip first word as it represents array offset
    // while bootloader expects only [len || data]
    let pubdata = ethabi::encode(&[ethabi::Token::Bytes(
        pubdata_information.build_pubdata(true),
    )])[32..]
        .to_vec();

    assert!(
        pubdata.len() / 32 <= OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS - 2,
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

use zksync_types::{
    event::L1MessengerL2ToL1Log,
    writes::{compress_state_diffs, StateDiffRecord},
};

/// Struct based on which the pubdata blob is formed
#[derive(Debug, Clone, Default)]
pub(crate) struct PubdataInput {
    pub(crate) user_logs: Vec<L1MessengerL2ToL1Log>,
    pub(crate) l2_to_l1_messages: Vec<Vec<u8>>,
    pub(crate) published_bytecodes: Vec<Vec<u8>>,
    pub(crate) state_diffs: Vec<StateDiffRecord>,
}

impl PubdataInput {
    pub(crate) fn build_pubdata(self, with_uncompressed_state_diffs: bool) -> Vec<u8> {
        let mut l1_messenger_pubdata = vec![];

        let PubdataInput {
            user_logs,
            l2_to_l1_messages,
            published_bytecodes,
            state_diffs,
        } = self;

        // Encoding user L2->L1 logs.
        // Format: `[(numberOfL2ToL1Logs as u32) || l2tol1logs[1] || ... || l2tol1logs[n]]`
        l1_messenger_pubdata.extend((user_logs.len() as u32).to_be_bytes());
        for l2tol1log in user_logs {
            l1_messenger_pubdata.extend(l2tol1log.packed_encoding());
        }

        // Encoding L2->L1 messages
        // Format: `[(numberOfMessages as u32) || (messages[1].len() as u32) || messages[1] || ... || (messages[n].len() as u32) || messages[n]]`
        l1_messenger_pubdata.extend((l2_to_l1_messages.len() as u32).to_be_bytes());
        for message in l2_to_l1_messages {
            l1_messenger_pubdata.extend((message.len() as u32).to_be_bytes());
            l1_messenger_pubdata.extend(message);
        }

        // Encoding bytecodes
        // Format: `[(numberOfBytecodes as u32) || (bytecodes[1].len() as u32) || bytecodes[1] || ... || (bytecodes[n].len() as u32) || bytecodes[n]]`
        l1_messenger_pubdata.extend((published_bytecodes.len() as u32).to_be_bytes());
        for bytecode in published_bytecodes {
            l1_messenger_pubdata.extend((bytecode.len() as u32).to_be_bytes());
            l1_messenger_pubdata.extend(bytecode);
        }

        // Encoding state diffs
        // Format: `[size of compressed state diffs u32 || compressed state diffs || (# state diffs: intial + repeated) as u32 || sorted state diffs by <index, address, key>]`
        let state_diffs_compressed = compress_state_diffs(state_diffs.clone());
        l1_messenger_pubdata.extend(state_diffs_compressed);

        if with_uncompressed_state_diffs {
            l1_messenger_pubdata.extend((state_diffs.len() as u32).to_be_bytes());
            for state_diff in state_diffs {
                l1_messenger_pubdata.extend(state_diff.encode_padded());
            }
        }

        l1_messenger_pubdata
    }
}

#[cfg(test)]
mod tests {
    use zksync_system_constants::{ACCOUNT_CODE_STORAGE_ADDRESS, BOOTLOADER_ADDRESS};
    use zksync_utils::u256_to_h256;

    use super::*;

    #[test]
    fn test_basic_pubdata_building() {
        // Just using some constant addresses for tests
        let addr1 = BOOTLOADER_ADDRESS;
        let addr2 = ACCOUNT_CODE_STORAGE_ADDRESS;

        let user_logs = vec![L1MessengerL2ToL1Log {
            l2_shard_id: 0,
            is_service: false,
            tx_number_in_block: 0,
            sender: addr1,
            key: 1.into(),
            value: 128.into(),
        }];

        let l2_to_l1_messages = vec![hex::decode("deadbeef").unwrap()];

        let published_bytecodes = vec![hex::decode("aaaabbbb").unwrap()];

        // For covering more cases, we have two state diffs:
        // One with enumeration index present (and so it is a repeated write) and the one without it.
        let state_diffs = vec![
            StateDiffRecord {
                address: addr2,
                key: 155.into(),
                derived_key: u256_to_h256(125.into()).0,
                enumeration_index: 12,
                initial_value: 11.into(),
                final_value: 12.into(),
            },
            StateDiffRecord {
                address: addr2,
                key: 156.into(),
                derived_key: u256_to_h256(126.into()).0,
                enumeration_index: 0,
                initial_value: 0.into(),
                final_value: 14.into(),
            },
        ];

        let input = PubdataInput {
            user_logs,
            l2_to_l1_messages,
            published_bytecodes,
            state_diffs,
        };

        let pubdata =
            ethabi::encode(&[ethabi::Token::Bytes(input.build_pubdata(true))])[32..].to_vec();

        assert_eq!(hex::encode(pubdata), "00000000000000000000000000000000000000000000000000000000000002c700000001000000000000000000000000000000000000000000008001000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000100000004deadbeef0000000100000004aaaabbbb0100002a040001000000000000000000000000000000000000000000000000000000000000007e090e0000000c0901000000020000000000000000000000000000000000008002000000000000000000000000000000000000000000000000000000000000009b000000000000000000000000000000000000000000000000000000000000007d000000000000000c000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008002000000000000000000000000000000000000000000000000000000000000009c000000000000000000000000000000000000000000000000000000000000007e00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
    }
}
