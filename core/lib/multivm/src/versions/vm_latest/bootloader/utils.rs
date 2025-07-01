use zksync_types::{ethabi, h256_to_u256, InteropRoot, ProtocolVersionId, U256};

use super::tx::BootloaderTx;
use crate::{
    interface::{
        pubdata::{PubdataBuilder, PubdataInput},
        BootloaderMemory, CompressedBytecodeInfo, TxExecutionMode,
    },
    utils::bytecode,
    vm_latest::{
        bootloader::l2_block::BootloaderL2Block,
        constants::{
            get_bootloader_tx_description_offset, get_compressed_bytecodes_offset,
            get_current_number_of_roots_in_block_offset, get_interop_blocks_begin_offset,
            get_interop_root_offset, get_operator_provided_l1_messenger_pubdata_offset,
            get_operator_refunds_offset, get_tx_description_offset,
            get_tx_operator_l2_block_info_offset, get_tx_overhead_offset,
            get_tx_trusted_gas_limit_offset, BOOTLOADER_TX_DESCRIPTION_SIZE,
            INTEROP_ROOT_SLOTS_SIZE, OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS,
            TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO,
        },
        MultiVmSubversion,
    },
};

pub struct InteropRootApplicationConfig {
    pub number_of_applied_interop_roots: usize,
    pub preexisting_blocks_number: usize,
}

pub(super) struct L2BlockApplicationConfig {
    pub tx_index: usize,
    pub start_new_l2_block: bool,
    pub subversion: MultiVmSubversion,
    pub interop_root_application_config: Option<InteropRootApplicationConfig>,
}

pub(super) fn get_memory_for_compressed_bytecodes(
    compressed_bytecodes: &[CompressedBytecodeInfo],
) -> Vec<U256> {
    let memory_addition: Vec<_> = compressed_bytecodes
        .iter()
        .flat_map(bytecode::encode_call)
        .collect();
    bytecode::bytes_to_be_words(&memory_addition)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_tx_to_memory(
    memory: &mut BootloaderMemory,
    bootloader_tx: &BootloaderTx,
    bootloader_l2_block: &BootloaderL2Block,
    tx_offset: usize,
    compressed_bytecodes_size: usize,
    execution_mode: TxExecutionMode,
    config: L2BlockApplicationConfig,
) -> (usize, usize) {
    let bootloader_description_offset = get_bootloader_tx_description_offset(config.subversion)
        + BOOTLOADER_TX_DESCRIPTION_SIZE * config.tx_index;
    let tx_description_offset = get_tx_description_offset(config.subversion) + tx_offset;

    memory.push((
        bootloader_description_offset,
        assemble_tx_meta(execution_mode, true),
    ));

    memory.push((
        bootloader_description_offset + 1,
        U256::from_big_endian(&(32 * tx_description_offset).to_be_bytes()),
    ));

    let refund_offset = get_operator_refunds_offset(config.subversion) + config.tx_index;
    memory.push((refund_offset, bootloader_tx.refund.into()));

    let overhead_offset = get_tx_overhead_offset(config.subversion) + config.tx_index;
    memory.push((overhead_offset, bootloader_tx.gas_overhead.into()));

    let trusted_gas_limit_offset =
        get_tx_trusted_gas_limit_offset(config.subversion) + config.tx_index;
    memory.push((trusted_gas_limit_offset, bootloader_tx.trusted_gas_limit));

    memory.extend(
        (tx_description_offset..tx_description_offset + bootloader_tx.encoded_len())
            .zip(bootloader_tx.encoded.clone()),
    );

    // Note, +1 is moving for pointer
    let compressed_bytecodes_offset =
        get_compressed_bytecodes_offset(config.subversion) + 1 + compressed_bytecodes_size;

    let applied_interop_roots = apply_l2_block_inner(memory, bootloader_l2_block, config);

    let encoded_compressed_bytecodes =
        get_memory_for_compressed_bytecodes(&bootloader_tx.compressed_bytecodes);
    let compressed_bytecodes_encoding = encoded_compressed_bytecodes.len();

    memory.extend(
        (compressed_bytecodes_offset
            ..compressed_bytecodes_offset + encoded_compressed_bytecodes.len())
            .zip(encoded_compressed_bytecodes),
    );

    (compressed_bytecodes_encoding, applied_interop_roots)
}

// Returns the number of applied interop roots
pub(crate) fn apply_l2_block(
    memory: &mut BootloaderMemory,
    bootloader_l2_block: &BootloaderL2Block,
    tx_index: usize,
    subversion: MultiVmSubversion,
    interop_root_application_config: Option<InteropRootApplicationConfig>,
) -> usize {
    apply_l2_block_inner(
        memory,
        bootloader_l2_block,
        L2BlockApplicationConfig {
            tx_index,
            start_new_l2_block: true,
            subversion,
            interop_root_application_config,
        },
    )
}

fn apply_l2_block_inner(
    memory: &mut BootloaderMemory,
    bootloader_l2_block: &BootloaderL2Block,
    config: L2BlockApplicationConfig,
) -> usize {
    // Since L2 block information start from the `TX_OPERATOR_L2_BLOCK_INFO_OFFSET` and each
    // L2 block info takes `TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO` slots, the position where the L2 block info
    // for this transaction needs to be written is:

    let block_position = get_tx_operator_l2_block_info_offset(config.subversion)
        + config.tx_index * TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO;

    memory.extend(vec![
        (block_position, bootloader_l2_block.number.into()),
        (block_position + 1, bootloader_l2_block.timestamp.into()),
        (
            block_position + 2,
            h256_to_u256(bootloader_l2_block.prev_block_hash),
        ),
        (
            block_position + 3,
            if config.start_new_l2_block {
                bootloader_l2_block.max_virtual_blocks_to_create.into()
            } else {
                U256::zero()
            },
        ),
    ]);

    if config.subversion != MultiVmSubversion::Interop
        || config.interop_root_application_config.is_none()
    {
        return 0;
    }

    let interop_root_application_config = config.interop_root_application_config.unwrap();
    for (offset, interop_root) in bootloader_l2_block.interop_roots.iter().enumerate() {
        apply_interop_root(
            memory,
            interop_root_application_config.number_of_applied_interop_roots + offset,
            interop_root.clone(),
            config.subversion,
            bootloader_l2_block.number,
        );
    }

    if !config.start_new_l2_block {
        return bootloader_l2_block.interop_roots.len();
    }

    apply_interop_root_number_in_block_number(
        memory,
        config.subversion,
        bootloader_l2_block.interop_roots.len(),
        interop_root_application_config.preexisting_blocks_number,
    );

    bootloader_l2_block.interop_roots.len()
}

fn apply_interop_root(
    memory: &mut BootloaderMemory,
    interop_root_offset: usize,
    interop_root: InteropRoot,
    subversion: MultiVmSubversion,
    l2_block_number: u32,
) {
    let interop_root_slot = get_interop_root_offset(subversion);
    // Convert the byte array into U256 words
    let mut u256_words: Vec<U256> = vec![
        U256::from(l2_block_number),
        U256::from(interop_root.chain_id),
        U256::from(interop_root.block_number),
        U256::from(interop_root.sides.len()),
    ];

    u256_words.extend(interop_root.sides);
    memory.extend(
        (interop_root_slot + interop_root_offset * INTEROP_ROOT_SLOTS_SIZE
            ..interop_root_slot + interop_root_offset * INTEROP_ROOT_SLOTS_SIZE + u256_words.len())
            .zip(u256_words),
    )
}

fn apply_interop_root_number_in_block_number(
    memory: &mut BootloaderMemory,
    subversion: MultiVmSubversion,
    number_of_interop_roots: usize,
    preexisting_blocks_number: usize,
) {
    let first_empty_slot = get_interop_blocks_begin_offset(subversion) + preexisting_blocks_number;
    let number_of_interop_roots_plus_one: U256 = (number_of_interop_roots + 1).into();
    memory.extend(vec![(first_empty_slot, number_of_interop_roots_plus_one)]);
    memory.extend(vec![(
        get_current_number_of_roots_in_block_offset(subversion),
        preexisting_blocks_number.into(),
    )]);
}

fn bootloader_memory_input(
    pubdata_builder: &dyn PubdataBuilder,
    input: &PubdataInput,
    protocol_version: ProtocolVersionId,
) -> Vec<u8> {
    let l2_da_validator_address = pubdata_builder.l2_da_validator();
    let operator_input = pubdata_builder.l1_messenger_operator_input(input, protocol_version);

    ethabi::encode(&[
        ethabi::Token::Address(l2_da_validator_address),
        ethabi::Token::Bytes(operator_input),
    ])
}

pub(crate) fn apply_pubdata_to_memory(
    memory: &mut BootloaderMemory,
    pubdata_builder: &dyn PubdataBuilder,
    pubdata_information: &PubdataInput,
    protocol_version: ProtocolVersionId,
    subversion: MultiVmSubversion,
) {
    let (l1_messenger_pubdata_start_slot, pubdata) = match subversion {
        MultiVmSubversion::SmallBootloaderMemory | MultiVmSubversion::IncreasedBootloaderMemory => {
            // Skipping two slots as they will be filled by the bootloader itself:
            // - One slot is for the selector of the call to the L1Messenger.
            // - The other slot is for the 0x20 offset for the calldata.
            let l1_messenger_pubdata_start_slot =
                get_operator_provided_l1_messenger_pubdata_offset(subversion) + 2;

            // Need to skip first word as it represents array offset
            // while bootloader expects only [len || data]
            let pubdata = ethabi::encode(&[ethabi::Token::Bytes(
                pubdata_builder.l1_messenger_operator_input(pubdata_information, protocol_version),
            )])[32..]
                .to_vec();

            assert!(
                pubdata.len() / 32 <= OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS - 2,
                "The encoded pubdata is too big"
            );

            (l1_messenger_pubdata_start_slot, pubdata)
        }
        MultiVmSubversion::Gateway
        | MultiVmSubversion::EvmEmulator
        | MultiVmSubversion::EcPrecompiles
        | MultiVmSubversion::Interop => {
            // Skipping the first slot as it will be filled by the bootloader itself:
            // It is for the selector of the call to the L1Messenger.
            let l1_messenger_pubdata_start_slot =
                get_operator_provided_l1_messenger_pubdata_offset(subversion) + 1;

            let pubdata =
                bootloader_memory_input(pubdata_builder, pubdata_information, protocol_version);

            assert!(
                // Note that unlike the previous version, the difference is `1`, since now it also includes the offset
                pubdata.len() / 32 < OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS,
                "The encoded pubdata is too big"
            );

            (l1_messenger_pubdata_start_slot, pubdata)
        }
    };

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
///   In the server, we may want to execute different parts of the transaction in the different context
///   For example, when checking validity, we don't want to actually execute transaction and have side effects.
///
///   Possible values:
///     - 0x00: validate & execute (normal mode)
///     - 0x02: execute but DO NOT validate
///
/// - 31 byte (LSB): whether to execute transaction or not (at all).
pub(super) fn assemble_tx_meta(execution_mode: TxExecutionMode, execute_tx: bool) -> U256 {
    let mut output = [0u8; 32];

    // Set 0 byte (execution mode)
    output[0] = match execution_mode {
        TxExecutionMode::VerifyExecute => 0x00,
        TxExecutionMode::EstimateFee => 0x00,
        TxExecutionMode::EthCall => 0x02,
    };

    // Set 31 byte (marker for tx execution)
    output[31] = u8::from(execute_tx);

    U256::from_big_endian(&output)
}
