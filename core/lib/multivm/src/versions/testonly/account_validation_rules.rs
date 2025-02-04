use assert_matches::assert_matches;
use zksync_test_contracts::TestContract;
use zksync_types::{address_to_h256, AccountTreeId, Address, StorageKey, H256};

use super::{
    get_empty_storage, require_eip712::make_aa_transaction, tester::VmTesterBuilder,
    ContractToDeploy, TestedVm, TestedVmForValidation,
};
use crate::interface::{
    tracer::ViolatedValidationRule, ExecutionResult, Halt, TxExecutionMode,
    VmExecutionResultAndLogs,
};

/// Checks that every limitation imposed on account validation results in an appropriate error.
/// The actual misbehavior cases are found in "validation-rule-breaker.sol".
pub(crate) fn test_account_validation_rules<VM: TestedVm + TestedVmForValidation>() {
    let (result, violated_rule) = test_rule::<VM>(0);
    assert!(!result.result.is_failed(), "{result:#?}");
    assert_matches!(violated_rule, None);

    let (result, violated_rule) = test_rule::<VM>(1);
    assert_matches!(
        &result.result,
        ExecutionResult::Halt {
            reason: Halt::TracerCustom(_)
        }
    );
    assert_matches!(
        violated_rule,
        Some(ViolatedValidationRule::TouchedDisallowedStorageSlots(_, _))
    );

    let (result, violated_rule) = test_rule::<VM>(2);
    assert_matches!(
        &result.result,
        ExecutionResult::Halt {
            reason: Halt::TracerCustom(_)
        }
    );
    assert_matches!(
        violated_rule,
        Some(ViolatedValidationRule::CalledContractWithNoCode(_))
    );

    let (result, violated_rule) = test_rule::<VM>(3);
    assert!(!result.result.is_failed(), "{result:#?}");
    assert_matches!(violated_rule, None);

    let (result, violated_rule) = test_rule::<VM>(4);
    assert_matches!(
        &result.result,
        ExecutionResult::Halt {
            reason: Halt::TracerCustom(_)
        }
    );
    assert_matches!(
        violated_rule,
        Some(ViolatedValidationRule::TookTooManyComputationalGas(_))
    );
}

fn test_rule<VM: TestedVm + TestedVmForValidation>(
    rule: u32,
) -> (VmExecutionResultAndLogs, Option<ViolatedValidationRule>) {
    let aa_address = Address::repeat_byte(0x10);
    let beneficiary_address = Address::repeat_byte(0x20);

    // Set the type of misbehaviour of the AA contract
    let mut storage_with_rule_break_set = get_empty_storage();
    storage_with_rule_break_set.set_value(
        StorageKey::new(AccountTreeId::new(aa_address), H256::zero()),
        H256::from_low_u64_be(rule.into()),
    );
    // Set the trusted address.
    storage_with_rule_break_set.set_value(
        StorageKey::new(AccountTreeId::new(aa_address), H256::from_low_u64_be(1)),
        address_to_h256(&Address::from_low_u64_be(0x800a)),
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

    let private_account = &mut vm.rich_accounts[0];
    let tx = make_aa_transaction(aa_address, beneficiary_address, private_account);
    vm.vm.run_validation(tx, 55)
}
