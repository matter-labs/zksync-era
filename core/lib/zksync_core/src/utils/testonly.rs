//! Test utils.

use zksync_contracts::BaseSystemContractsHashes;
use zksync_system_constants::ZKPORTER_IS_AVAILABLE;
use zksync_types::{
    block::{L1BatchHeader, MiniblockHeader},
    commitment::{L1BatchMetaParameters, L1BatchMetadata},
    fee::Fee,
    fee_model::BatchFeeInput,
    l2::L2Tx,
    transaction_request::PaymasterParams,
    Address, L1BatchNumber, L2ChainId, MiniblockNumber, Nonce, ProtocolVersionId, H256, U256,
};

/// Creates a miniblock header with the specified number and deterministic contents.
pub(crate) fn create_miniblock(number: u32) -> MiniblockHeader {
    MiniblockHeader {
        number: MiniblockNumber(number),
        timestamp: number.into(),
        hash: H256::from_low_u64_be(number.into()),
        l1_tx_count: 0,
        l2_tx_count: 0,
        base_fee_per_gas: 100,
        batch_fee_input: BatchFeeInput::l1_pegged(100, 100),
        base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        virtual_blocks: 1,
    }
}

/// Creates an L1 batch header with the specified number and deterministic contents.
pub(crate) fn create_l1_batch(number: u32) -> L1BatchHeader {
    let mut header = L1BatchHeader::new(
        L1BatchNumber(number),
        number.into(),
        Address::default(),
        BaseSystemContractsHashes::default(),
        ProtocolVersionId::latest(),
    );
    header.is_finished = true;
    header
}

/// Creates metadata for an L1 batch with the specified number.
pub(crate) fn create_l1_batch_metadata(number: u32) -> L1BatchMetadata {
    L1BatchMetadata {
        root_hash: H256::from_low_u64_be(number.into()),
        rollup_last_leaf_index: u64::from(number) + 20,
        merkle_root_hash: H256::from_low_u64_be(number.into()),
        initial_writes_compressed: vec![],
        repeated_writes_compressed: vec![],
        commitment: H256::from_low_u64_be(number.into()),
        l2_l1_messages_compressed: vec![],
        l2_l1_merkle_root: H256::from_low_u64_be(number.into()),
        block_meta_params: L1BatchMetaParameters {
            zkporter_is_available: ZKPORTER_IS_AVAILABLE,
            bootloader_code_hash: BaseSystemContractsHashes::default().bootloader,
            default_aa_code_hash: BaseSystemContractsHashes::default().default_aa,
        },
        aux_data_hash: H256::zero(),
        meta_parameters_hash: H256::zero(),
        pass_through_data_hash: H256::zero(),
        events_queue_commitment: Some(H256::zero()),
        bootloader_initial_content_commitment: Some(H256::zero()),
        state_diffs_compressed: vec![],
    }
}

/// Creates an L2 transaction with randomized parameters.
pub(crate) fn create_l2_transaction(fee_per_gas: u64, gas_per_pubdata: u32) -> L2Tx {
    let fee = Fee {
        gas_limit: 1000_u64.into(),
        max_fee_per_gas: fee_per_gas.into(),
        max_priority_fee_per_gas: 0_u64.into(),
        gas_per_pubdata_limit: gas_per_pubdata.into(),
    };
    let mut tx = L2Tx::new_signed(
        Address::random(),
        vec![],
        Nonce(0),
        fee,
        U256::zero(),
        L2ChainId::from(271),
        &H256::random(),
        None,
        PaymasterParams::default(),
    )
    .unwrap();
    // Input means all transaction data (NOT calldata, but all tx fields) that came from the API.
    // This input will be used for the derivation of the tx hash, so put some random to it to be sure
    // that the transaction hash is unique.
    tx.set_input(H256::random().0.to_vec(), H256::random());
    tx
}
