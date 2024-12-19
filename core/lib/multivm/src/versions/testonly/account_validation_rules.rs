use assert_matches::assert_matches;
use zksync_test_contracts::TestContract;
use zksync_types::{u256_to_h256, AccountTreeId, Address, StorageKey};
use zksync_vm_interface::tracer::ViolatedValidationRule;

use super::{
    get_empty_storage, require_eip712::make_aa_transaction, tester::VmTesterBuilder,
    ContractToDeploy, TestedVm, TestedVmForValidation,
};
use crate::interface::TxExecutionMode;

/// Checks that every limitation imposed on account validation results in an appropriate error.
/// The actual misbehavior cases are found in "validation-rule-breaker.sol".
pub(crate) fn test_account_validation_rules<VM: TestedVm + TestedVmForValidation>() {
    assert_matches!(test_rule::<VM>(0), None);
    assert_matches!(
        test_rule::<VM>(1),
        Some(ViolatedValidationRule::TouchedDisallowedStorageSlots(_, _))
    );
    assert_matches!(
        test_rule::<VM>(2),
        Some(ViolatedValidationRule::CalledContractWithNoCode(_))
    );
    assert_matches!(test_rule::<VM>(3), None);
    assert_matches!(
        test_rule::<VM>(4),
        Some(ViolatedValidationRule::TookTooManyComputationalGas(_))
    )
}

fn test_rule<VM: TestedVm + TestedVmForValidation>(rule: u32) -> Option<ViolatedValidationRule> {
    let aa_address = Address::repeat_byte(0x10);
    let beneficiary_address = Address::repeat_byte(0x20);

    // Set the type of misbehaviour of the AA contract
    let mut storage_with_rule_break_set = get_empty_storage();
    storage_with_rule_break_set.set_value(
        StorageKey::new(AccountTreeId::new(aa_address), u256_to_h256(0.into())),
        u256_to_h256(rule.into()),
    );

    let bytecode = TestContract::validation_test().bytecode.to_vec();
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_custom_contracts(vec![
            ContractToDeploy::account(bytecode, aa_address).funded()
        ])
        .with_storage(storage_with_rule_break_set)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let private_account = vm.rich_accounts[0].clone();

    vm.vm.run_validation(
        make_aa_transaction(aa_address, beneficiary_address, &private_account),
        55,
    )
}
