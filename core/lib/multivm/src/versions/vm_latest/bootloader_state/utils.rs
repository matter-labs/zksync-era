use zksync_types::{ethabi, U256};
use zksync_utils::{bytecode::CompressedBytecodeInfo, bytes_to_be_words, h256_to_u256};

use super::tx::BootloaderTx;
use crate::{
    interface::{
        types::inputs::system_env::{PubdataParams, PubdataType},
        BootloaderMemory, TxExecutionMode,
    },
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

pub(crate) fn get_encoded_pubdata(
    pubdata_information: PubdataInput,
    pubdata_params: PubdataParams,
    l2_version: bool,
) -> Vec<u8> {
    let pubdata_bytes: Vec<u8> = if pubdata_params.pubdata_type == PubdataType::Rollup {
        RollupPubdataBuilder::new().build_pubdata(pubdata_information, l2_version)
    } else {
        ValidiumPubdataBuilder::new().build_pubdata(pubdata_information, l2_version)
    };

    let pubdata = if l2_version {
        ethabi::encode(&[
            ethabi::Token::Address(pubdata_params.l2_da_validator_address),
            ethabi::Token::Bytes(pubdata_bytes),
        ])
        .to_vec()
    } else {
        pubdata_bytes
    };

    pubdata
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
        pubdata.len() / 32 <= OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS - 1,
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

/*


000000070001000000000000000000000000000000000000000080010b198075f23eba8137d7c071e5b9e594a4acabb85dfbd59b4b5dd326a54671ed000000000000000000000000000000000000000000000000000000000000000100010001000000000000000000000000000000000000800191a487c94f7f5752ba5b22e66ec908a18459fdab5afd7f784d3ebec26825297900000000000000000000000000000000000000000000000000000000000000010001000200000000000000000000000000000000000080012e02113ea86c32bad4f949c1fd59c2bdce8bfef509c1099fd4c3ca2e566f6e58000000000000000000000000000000000000000000000000000000000000000100010003000000000000000000000000000000000000800149c5d8c0374361254dcefb02ce84558490e44918871ed28cd357a339cdeefe080000000000000000000000000000000000000000000000000000000000000001000100040000000000000000000000000000000000008001af31ea7528ea3e2304cc96df59e20d313952f5a310c1935e7c611d76da7daf9a0000000000000000000000000000000000000000000000000000000000000001000100050000000000000000000000000000000000008001cbbd9fab11daca9871dca880c0afdf4a642065726c96ea4e459b2e5741e03bb3000000000000000000000000000000000000000000000000000000000000000100010006000000000000000000000000000000000000800147c334b4ca442a98afc204272c1782dd8943b392ff9c75f39eaccaa6d7a904d4000000000000000000000000000000000000000000000000000000000000000100000000000000000100079404002850067c298622a3e4cfa9cde6ecff67aa2da9f870ca7c0ccb2f434dc1df65b16b00010000cbacfe9317411fc97202d4074acbaebf0032eceeffd580e4a8a7e587294de82279a36384872d4741f8aa233d2405fac8afb52ed756be0032fe3d88e76a00010000691fa4f751f8312bc555242f18ed78cdc9aabc0ea77d7d5a675ee8ac6f2a225450f25463492d6fe0d547fb2b2d4e4536217887eedc183f05e90045e9b100010001f909cf8e16ddeb08c19965b7d1f362dc33bd94a9fd3fc282daa9f6deb970216b95d44fa73bdedece86588381e550661215b6f8818158c4b3d5be8e83cc000100015d49d29767b868317d524a1ef8985652932d4be89964953a01931026c9f257c141880a838c0ffbdfd490de5f14c3458313ea387e0c3a2d39c6296756fd00010001915d77524723b4d66b72f62b3390d4875e0845b432d12440003344561699a07322a94f84dc86f56e387ef80f6a38ba5fb276dd7e4674434e97c3459a32000100047921bf49b783c0687b9c1594366171a3ba63b916e5894a00fa1184bdf97e44a311e3a4f4c0db49b982086e38fec6b9911f5e868aa999953f2b1c363e57000100005b75ef1bed7be47d27d372a69fbeac5eb64a51c47efd6b1d488408c2a8de18b33c44fb7fb3975d75eaa7596e22f54243d0c079fd59746a29e84d441d9a890200000000000000000000000000000000ab63c4cebbd508a7d7184f0b9134453eea7a09ca749610d5576f8046241b9cde8905000000000000000000000000000000008edd044ac3e812ea7a42a2bdb3e580b6d1e581e5ebbb21368f9570972380e7320901c90cc02810b9c5f7af23337717fd255535f4a0f8b918479cadf2126802cde5280901c904134d8e764daff1a3530b1986b7404c809fff598dc906043a1bfe135f5d550901a161b238d32452c5f2cc28dc43be48ed11a89e47bcaa1ba8879814c16d72b465090135c315a518cbff58da6290dd2f9a2e6598431133b4ea5c3f837cee83a25bcfcd0901f3fb235c63b612eb7c0c5739451af0fdc5f014a19148054a74ba704a3f4645df0901d684c68a9ac5173e43b36e1af8954c64576336d871ae20316f848d7d1432c3500901eba9d3385189120a4f2933cebb8233c028f22dbc71ed5c2daeb93cfe02d7c4cb0901ec0ca8527782cc4883763fe261fe5882cc50c7badf137063223edd516ed11f8b090971e91721f9918576d760f02f03cac47c6f4003316031848e3c1d99e6e83a474341019c7855ba6c40002d5a6962cccee5d4adb48a36bbbf443a531721484381125937f3001ac5ff875b3901fafd77b549307b727eb806c926fb20c8ad087c57422977cebd06373e26d19b640e5fe32c85ff41019a7d5842b6f6d0a25420c1d9d705358c134cc601d9d184cb4dfdde7e1cac2bc3d4d38bf9ec44e62105f5e100123bafc586f77764488cd24c6a77546e5a0fe8bdfb4fa203cfaffc36cce4dd5b89010000000000000000000000006672c0d68e7dd06ac5b73b473be6bc5a51030f4c7437657cb7b29bf376c564b8d1675a5e89020000000000000000000000006672c0d74ba84e1f37d041bc6e55ba396826cc494e84d4815b6db52690422eea7386314f00e8e77626586f73b955364c7b4bbf0bb7f7685ebd40e852b164633a4acbd3244c3de2202ccb626ad387d70722e64fbe44562e2f231a290c08532b8d6aba402ff500e841cda80d6b1bb8bb89a856180bc5f798d0226fe774177d472bf694df6eaffb79093588d0e847efa73a10ce20e4799fb1e46642d65617c7e5213fa04989d92d89020000000000000000000000006672c0d787ded247e1660f827071c7f1371934589751085384fc9f4462c1f1897c5c3eef89010000000000000000000000000000000186248193eb4dd2a8ce815f876c124d48359522f0854d95d8072eaff0d37d55bd1103203e890d6c2c3bada6eecc9603a99c1c6259ed5a6402f1c76cc18b568c3aefba0f110574911dd2ad743ff237d411648a0fe32c6d74eec060716a2a74352f6b1c435b5d67000ed3937da1db366021236aa52cc275d78bd86e7c2f3aec292dbdbab2869a26096d9b9e771d8f017ec4f95c0e11c75e34f2e97e5efc3125ba2dacd5a9fe4eed40a15d5c5c8a659d23ec123a97cad75374aab7f488e4b076e3db6d68ad1addeae41f91a7bfd0909255780fc3c9c5d8d1c5cbd3aa5475a1f9c5845988bc77f85525b3ff26eb71c9637f17f24fb16b1fbf16d9c14f6fba40896c133402901cf21d9906dc69b8ecedec157b3309ff5df26ae3ad00b027ed58df210d8fa50ec2ce7cd3424d3bd362699a756b556e4ab1c3d06996f2d3da2a019c6ec38d90b6e2507a24650002438e7919cb121fe8ac4de85497120fafbfb18380e9a621b91ec0ec82abad95f4a15378ebf01055f32f8dceab8f0fb7dab7f367668fd62e48fad003b37cc09ca8ac4dc5f94db70c8e86ef476bdca72197737d30371b00010001211b0c33353cdf7a320f768e3dc40bce1326d639fcac099bba9ecd8e34a0e0abcab8da51739d794f8737ff1b7bb5b7decdd5a0b56d92616f23fe4a3d1aa19cdd75c874579afee6be7a2ae0287baa91549be3b884b5a113e1769aa8dacb4dbf3e48f780f3e4aa2246513d12c2c9129b1e7e7aa15d5c5c8a659d23ec123a97cad75374aab7f488e42e215fe95bb6101577f142cc1cafb0c790181a453a617f3dbdef8020550c3de509ff00000000000000000000000000000000000000

0x
5569ff0d935d48dcff81a29257bf8f56a403eb1e435bbe2bb8cf41609b179e6e
79571209c061144fd07ac6194590f5fe6e346ec8db1e6ee8717efac80b91395d
01
45645aa66ee65bb554c4c60992c9f2dbd16cfd9c3e9671158ce8cf844acce97e
c57b443e75f925040c0a2f438538f5b50d171a9295ac2dced882334aeb35d77942e14c0386a29c697c1899bd5a5b37e5ab6259b15500c26aa54d35c93253f6168360751c4e04f4a3cfd3d0fbe10873a30bc1ee43d0011170599144c136e23db295296bcaf1f1192789293b6b3b29e6bfa6c4c0d7786ef0b3a2e5eca3e349498bfee1754b590d6e62802a9662f4a6a327

*/
