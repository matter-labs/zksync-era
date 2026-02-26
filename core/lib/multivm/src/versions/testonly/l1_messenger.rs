use std::rc::Rc;

use ethabi::Token;
use zksync_contracts::l1_messenger_contract;
use zksync_test_contracts::{TestContract, TxType};
use zksync_types::{
    commitment::{L2DACommitmentScheme, L2PubdataValidator},
    u256_to_h256,
    web3::keccak256,
    Address, Execute, ProtocolVersionId, H256, L1_MESSENGER_ADDRESS, U256,
};

use super::{TestedVm, VmTesterBuilder};
use crate::{
    interface::{
        pubdata::{PubdataBuilder, PubdataInput},
        InspectExecutionMode, TxExecutionMode, VmInterfaceExt,
    },
    pubdata_builders::FullPubdataBuilder,
    vm_latest::constants::ZK_SYNC_BYTES_PER_BLOB,
};

const L2_DA_VALIDATOR_OUTPUT_HASH_KEY: usize = 5;
const USED_L2_DA_VALIDATOR_ADDRESS_KEY: usize = 6;

fn encoded_uncompressed_state_diffs(input: &PubdataInput) -> Vec<u8> {
    let mut result = vec![];
    for state_diff in input.state_diffs.iter() {
        result.extend(state_diff.encode_padded());
    }
    result
}

fn compose_header_for_l1_commit_rollup(input: PubdataInput) -> Vec<u8> {
    // The preimage under the hash `l2DAValidatorOutputHash` is expected to be in the following format:
    // - First 32 bytes are the hash of the uncompressed state diff.
    // - Then, there is a 32-byte hash of the full pubdata.
    // - Then, there is the 1-byte number of blobs published.
    // - Then, there are linear hashes of the published blobs, 32 bytes each.

    let mut full_header = vec![];

    let uncompressed_state_diffs = encoded_uncompressed_state_diffs(&input);
    let uncompressed_state_diffs_hash = keccak256(&uncompressed_state_diffs);
    full_header.extend(uncompressed_state_diffs_hash);
    let pubdata_builder = FullPubdataBuilder::new(L2PubdataValidator::Address(Address::zero()));

    let mut full_pubdata =
        pubdata_builder.settlement_layer_pubdata(&input, ProtocolVersionId::latest());
    let full_pubdata_hash = keccak256(&full_pubdata);
    full_header.extend(full_pubdata_hash);

    // Now, we need to calculate the linear hashes of the blobs.
    // Firstly, let's pad the pubdata to the size of the blob.
    if full_pubdata.len() % ZK_SYNC_BYTES_PER_BLOB != 0 {
        full_pubdata.resize(
            full_pubdata.len() + ZK_SYNC_BYTES_PER_BLOB
                - full_pubdata.len() % ZK_SYNC_BYTES_PER_BLOB,
            0,
        );
    }
    full_header.push((full_pubdata.len() / ZK_SYNC_BYTES_PER_BLOB) as u8);

    full_pubdata
        .chunks(ZK_SYNC_BYTES_PER_BLOB)
        .for_each(|chunk| {
            full_header.extend(keccak256(chunk));
        });

    full_header
}

pub(crate) fn test_rollup_da_output_hash_match<VM: TestedVm>() {
    // In this test, we check whether the L2 DA output hash is as expected.

    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];

    // Firstly, deploy tx. It should publish the bytecode of the "test contract"
    let counter_bytecode = TestContract::counter().bytecode;
    let tx = account.get_deploy_tx(counter_bytecode, None, TxType::L2).tx;
    // We do not use compression here, to have the bytecode published in full.
    let (_, result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, false);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");

    // Then, we call the l1 messenger to also send an L2->L1 message.
    let l1_messenger_contract = l1_messenger_contract();
    let encoded_data = l1_messenger_contract
        .function("sendToL1")
        .unwrap()
        .encode_input(&[Token::Bytes(vec![])])
        .unwrap();

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(L1_MESSENGER_ADDRESS),
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");

    let pubdata_builder = FullPubdataBuilder::new(L2PubdataValidator::CommitmentScheme(
        L2DACommitmentScheme::BlobsAndPubdataKeccak256,
    ));
    let batch_result = vm.vm.finish_batch(Rc::new(pubdata_builder));
    assert!(
        !batch_result.block_tip_execution_result.result.is_failed(),
        "Transaction wasn't successful {:?}",
        batch_result.block_tip_execution_result.result
    );
    let pubdata_input = vm.vm.pubdata_input();

    // Just to double check that the test makes sense.
    assert!(!pubdata_input.user_logs.is_empty());
    assert!(!pubdata_input.l2_to_l1_messages.is_empty());
    assert!(!pubdata_input.published_bytecodes.is_empty());
    assert!(!pubdata_input.state_diffs.is_empty());

    let expected_header: Vec<u8> = compose_header_for_l1_commit_rollup(pubdata_input);

    let l2_da_validator_output_hash = batch_result
        .block_tip_execution_result
        .logs
        .system_l2_to_l1_logs
        .iter()
        .find(|log| log.0.key == u256_to_h256(L2_DA_VALIDATOR_OUTPUT_HASH_KEY.into()))
        .unwrap()
        .0
        .value;

    assert_eq!(
        l2_da_validator_output_hash,
        keccak256(&expected_header).into()
    );

    let da_commitment_scheme = batch_result
        .block_tip_execution_result
        .logs
        .system_l2_to_l1_logs
        .iter()
        .find(|log| log.0.key == u256_to_h256(USED_L2_DA_VALIDATOR_ADDRESS_KEY.into()))
        .unwrap()
        .0
        .value;

    assert_eq!(
        da_commitment_scheme,
        H256::from_low_u64_be(L2DACommitmentScheme::BlobsAndPubdataKeccak256 as u64)
    );
}
