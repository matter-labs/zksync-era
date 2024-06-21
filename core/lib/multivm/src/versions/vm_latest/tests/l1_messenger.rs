use ethabi::Token;
use zksync_contracts::l1_messenger_contract;
use zksync_types::{web3::keccak256, Execute, L1_MESSENGER_ADDRESS, U256};
use zksync_utils::{address_to_h256, u256_to_h256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        constants::{
            L2_DA_VALIDATOR_OUTPUT_HASH_KEY, USED_L2_DA_VALIDATOR_ADDRESS_KEY,
            ZK_SYNC_BYTES_PER_BLOB,
        },
        tests::{
            tester::{DeployContractsTx, TxType, VmTesterBuilder},
            utils::read_test_contract,
        },
        types::internals::{
            pubdata::{PubdataBuilder, RollupPubdataBuilder},
            PubdataInput,
        },
        HistoryEnabled,
    },
};

pub fn compose_header_for_l1_commit_rollup(input: PubdataInput) -> Vec<u8> {
    // The preimage under the hash `l2DAValidatorOutputHash` is expected to be in the following format:
    // - First 32 bytes are the hash of the uncompressed state diff.
    // - Then, there is a 32-byte hash of the full pubdata.
    // - Then, there is the 1-byte number of blobs published.
    // - Then, there are linear hashes of the published blobs, 32 bytes each.

    let mut full_header = vec![];

    let uncompressed_state_diffs = input.encoded_uncompressed_state_diffs();
    let uncompressed_state_diffs_hash = keccak256(&uncompressed_state_diffs);
    full_header.extend(uncompressed_state_diffs_hash);

    let mut full_pubdata = RollupPubdataBuilder::new().build_pubdata(input, false);
    let full_pubdata_hash = keccak256(&full_pubdata);
    full_header.extend(full_pubdata_hash);

    // Now, we need to calculate the linear hashes of the blobs.
    // Firstly, let's pad the pubdata to the size of the blob.
    if full_pubdata.len() % ZK_SYNC_BYTES_PER_BLOB != 0 {
        let padding =
            vec![0u8; ZK_SYNC_BYTES_PER_BLOB - full_pubdata.len() % ZK_SYNC_BYTES_PER_BLOB];
        full_pubdata.extend(padding);
    }
    full_header.push((full_pubdata.len() / ZK_SYNC_BYTES_PER_BLOB) as u8);

    full_pubdata
        .chunks(ZK_SYNC_BYTES_PER_BLOB)
        .into_iter()
        .for_each(|chunk| {
            full_header.extend(keccak256(chunk));
        });

    full_header
}

#[test]
fn test_publish_and_clear_state() {
    // In this test, we check whether the L2 DA output hash is as expected.
    // We will publish 320kb worth of pubdata.
    // It should produce 3 blobs.

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    // Firstly, deploy tx. It should publish the bytecode of the "test contract"
    let counter = read_test_contract();
    let account = &mut vm.rich_accounts[0];
    let DeployContractsTx { tx, .. } = account.get_deploy_tx(&counter, None, TxType::L2);
    // We do not use compression here, to have the bytecode published in full.
    vm.vm.push_transaction_with_compression(tx, false);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
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
            contract_address: L1_MESSENGER_ADDRESS,
            calldata: encoded_data,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");

    let batch_result = vm.vm.execute(VmExecutionMode::Batch);
    if batch_result.result.is_failed() {
        panic!("Batch execution failed: {:?}", batch_result.result);
    }
    assert!(
        !batch_result.result.is_failed(),
        "Transaction wasn't successful"
    );
    let pubdata_input = vm.vm.bootloader_state.get_pubdata_information().clone();

    // Just to double check that the test makes sense.
    assert!(!pubdata_input.user_logs.is_empty());
    assert!(!pubdata_input.l2_to_l1_messages.is_empty());
    assert!(!pubdata_input.published_bytecodes.is_empty());
    assert!(!pubdata_input.state_diffs.is_empty());

    let expected_header: Vec<u8> = compose_header_for_l1_commit_rollup(pubdata_input);

    let l2_da_validator_output_hash = batch_result
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

    let l2_used_da_validator_address = batch_result
        .logs
        .system_l2_to_l1_logs
        .iter()
        .find(|log| log.0.key == u256_to_h256(USED_L2_DA_VALIDATOR_ADDRESS_KEY.into()))
        .unwrap()
        .0
        .value;

    assert_eq!(
        l2_used_da_validator_address,
        address_to_h256(&vm.vm.system_env.pubdata_params.l2_da_validator_address)
    );
}
