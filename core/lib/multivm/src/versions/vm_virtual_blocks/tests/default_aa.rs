use zksync_system_constants::L2_ETH_TOKEN_ADDRESS;
use zksync_types::system_contracts::{DEPLOYMENT_NONCE_INCREMENT, TX_NONCE_INCREMENT};

use zksync_types::{get_code_key, get_known_code_key, get_nonce_key, AccountTreeId, U256};
use zksync_utils::u256_to_h256;

use crate::interface::{TxExecutionMode, VmExecutionMode, VmInterface};
use crate::vm_latest::HistoryEnabled;
use crate::vm_virtual_blocks::tests::tester::{DeployContractsTx, TxType, VmTesterBuilder};
use crate::vm_virtual_blocks::tests::utils::{
    get_balance, read_test_contract, verify_required_storage,
};

#[test]
fn test_default_aa_interaction() {
    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let counter = read_test_contract();
    let account = &mut vm.rich_accounts[0];
    let DeployContractsTx {
        tx,
        bytecode_hash,
        address,
    } = account.get_deploy_tx(&counter, None, TxType::L2);
    let maximal_fee = tx.gas_limit() * vm.vm.batch_env.base_fee();

    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");

    vm.vm.execute(VmExecutionMode::Batch);
    vm.vm.get_current_execution_state();

    // Both deployment and ordinary nonce should be incremented by one.
    let account_nonce_key = get_nonce_key(&account.address);
    let expected_nonce = TX_NONCE_INCREMENT + DEPLOYMENT_NONCE_INCREMENT;

    // The code hash of the deployed contract should be marked as republished.
    let known_codes_key = get_known_code_key(&bytecode_hash);

    // The contract should be deployed successfully.
    let account_code_key = get_code_key(&address);

    let expected_slots = vec![
        (u256_to_h256(expected_nonce), account_nonce_key),
        (u256_to_h256(U256::from(1u32)), known_codes_key),
        (bytecode_hash, account_code_key),
    ];

    verify_required_storage(&vm.vm.state, expected_slots);

    let expected_fee = maximal_fee
        - U256::from(result.refunds.gas_refunded) * U256::from(vm.vm.batch_env.base_fee());
    let operator_balance = get_balance(
        AccountTreeId::new(L2_ETH_TOKEN_ADDRESS),
        &vm.fee_account,
        vm.vm.state.storage.storage.get_ptr(),
    );

    assert_eq!(
        operator_balance, expected_fee,
        "Operator did not receive his fee"
    );
}
