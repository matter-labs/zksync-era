use zksync_test_account::{DeployContractsTx, TxType};
use zksync_types::{
    get_code_key, get_known_code_key, get_nonce_key,
    system_contracts::{DEPLOYMENT_NONCE_INCREMENT, TX_NONCE_INCREMENT},
    utils::storage_key_for_eth_balance,
    U256,
};
use zksync_utils::h256_to_u256;

use super::{default_pubdata_builder, read_test_contract, tester::VmTesterBuilder, TestedVm};
use crate::{
    interface::{InspectExecutionMode, TxExecutionMode, VmInterfaceExt},
    vm_latest::utils::fee::get_batch_base_fee,
};

pub(crate) fn test_default_aa_interaction<VM: TestedVm>() {
    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let counter = read_test_contract();
    let account = &mut vm.rich_accounts[0];
    let DeployContractsTx {
        tx,
        bytecode_hash,
        address,
    } = account.get_deploy_tx(&counter, None, TxType::L2);
    let maximal_fee = tx.gas_limit() * get_batch_base_fee(&vm.l1_batch_env);

    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");

    vm.vm.finish_batch(default_pubdata_builder());

    vm.vm.get_current_execution_state();

    // Both deployment and ordinary nonce should be incremented by one.
    let account_nonce_key = get_nonce_key(&account.address);
    let expected_nonce = TX_NONCE_INCREMENT + DEPLOYMENT_NONCE_INCREMENT;

    // The code hash of the deployed contract should be marked as republished.
    let known_codes_key = get_known_code_key(&bytecode_hash);

    // The contract should be deployed successfully.
    let account_code_key = get_code_key(&address);

    let operator_balance_key = storage_key_for_eth_balance(&vm.l1_batch_env.fee_account);
    let expected_fee = maximal_fee
        - U256::from(result.refunds.gas_refunded)
            * U256::from(get_batch_base_fee(&vm.l1_batch_env));

    let expected_slots = [
        (account_nonce_key, expected_nonce),
        (known_codes_key, 1.into()),
        (account_code_key, h256_to_u256(bytecode_hash)),
        (operator_balance_key, expected_fee),
    ];
    vm.vm.verify_required_storage(&expected_slots);
}
