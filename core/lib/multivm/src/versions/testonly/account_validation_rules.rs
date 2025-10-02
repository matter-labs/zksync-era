use assert_matches::assert_matches;
use zksync_test_contracts::{Account, TestContract};
use zksync_types::{address_to_h256, fee::Fee, AccountTreeId, Address, StorageKey, H256};

use super::{
    default_system_env, get_empty_storage, require_eip712::make_aa_transaction,
    tester::VmTesterBuilder, ContractToDeploy, TestedVm, TestedVmForValidation,
};
use crate::interface::{
    tracer::ViolatedValidationRule, ExecutionResult, Halt, InspectExecutionMode, SystemEnv,
    TxExecutionMode, VmExecutionResultAndLogs, VmInterfaceExt,
};

/// Corresponds to test cases in the `ValidationRuleBreaker` contract.
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
enum TestCase {
    Baseline = 0,
    ReadBootloaderBalance = 1,
    CallEoa = 2,
    ReadFromTrustedAddressSlot = 3,
    RecursiveOutOfGas = 4,
    PlainOutOfGas = 5,
    PlainOutOfGasWithCatch = 6,
    ReadFromMappings = 7,
    DisallowedReadFromLayeredMapping = 8,
    DisallowedReadFromMappingWithOffset = 9,
    ReadFromMappingWithLargeOffset = 10,
}

/// Checks that every limitation imposed on account validation results in an appropriate error.
/// The actual misbehavior cases are found in "validation-rule-breaker.sol".
pub(crate) fn test_account_validation_rules<VM: TestedVm + TestedVmForValidation>() {
    let (result, violated_rule) = test_rule::<VM>(u32::MAX, TestCase::Baseline);
    assert!(!result.result.is_failed(), "{result:#?}");
    assert_matches!(violated_rule, None);

    let (result, violated_rule) = test_rule::<VM>(u32::MAX, TestCase::ReadBootloaderBalance);
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

    let (result, violated_rule) = test_rule::<VM>(u32::MAX, TestCase::CallEoa);
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

    let (result, violated_rule) = test_rule::<VM>(u32::MAX, TestCase::ReadFromTrustedAddressSlot);
    assert!(!result.result.is_failed(), "{result:#?}");
    assert_matches!(violated_rule, None);

    for test_case in [TestCase::RecursiveOutOfGas, TestCase::PlainOutOfGas] {
        let (result, violated_rule) = test_rule::<VM>(u32::MAX, test_case);
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

    let (result, violated_rule) = test_rule::<VM>(u32::MAX, TestCase::ReadFromMappings);
    assert!(!result.result.is_failed(), "{result:#?}");
    assert_matches!(violated_rule, None);

    for test_case in [
        TestCase::DisallowedReadFromLayeredMapping,
        TestCase::DisallowedReadFromMappingWithOffset,
        TestCase::ReadFromMappingWithLargeOffset,
    ] {
        println!("Testing case: {test_case:?}");

        let (result, violated_rule) = test_rule::<VM>(u32::MAX, test_case);
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
    }
}

fn test_rule<VM: TestedVmForValidation>(
    validation_gas_limit: u32,
    test_case: TestCase,
) -> (VmExecutionResultAndLogs, Option<ViolatedValidationRule>) {
    let aa_address = Address::repeat_byte(0x10);
    let beneficiary_address = Address::repeat_byte(0x20);

    // Set the type of misbehaviour of the AA contract
    let mut storage_with_rule_break_set = get_empty_storage();
    storage_with_rule_break_set.set_value(
        StorageKey::new(AccountTreeId::new(aa_address), H256::zero()),
        H256::from_low_u64_be(test_case as u64),
    );
    // Set the trusted address.
    storage_with_rule_break_set.set_value(
        StorageKey::new(AccountTreeId::new(aa_address), H256::from_low_u64_be(1)),
        address_to_h256(&Address::from_low_u64_be(0x800a)),
    );

    let bytecode = TestContract::validation_test().bytecode.to_vec();
    let mut contracts = vec![ContractToDeploy::account(bytecode, aa_address).funded()];

    if matches!(
        test_case,
        TestCase::ReadFromMappings
            | TestCase::DisallowedReadFromLayeredMapping
            | TestCase::DisallowedReadFromMappingWithOffset
            | TestCase::ReadFromMappingWithLargeOffset
    ) {
        let token_address = Address::repeat_byte(0x23);
        let bytecode = TestContract::validation_test_mock_token().bytecode.to_vec();
        contracts.push(ContractToDeploy::new(bytecode, token_address));
        // Set the mock token address in the AA contract.
        storage_with_rule_break_set.set_value(
            StorageKey::new(AccountTreeId::new(aa_address), H256::from_low_u64_be(2)),
            address_to_h256(&token_address),
        );
    }

    let mut vm = VmTesterBuilder::new()
        .with_system_env(SystemEnv {
            default_validation_computational_gas_limit: validation_gas_limit,
            ..default_system_env()
        })
        .with_custom_contracts(contracts)
        .with_storage(storage_with_rule_break_set)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let private_account = &mut vm.rich_accounts[0];
    let tx = make_aa_transaction(aa_address, beneficiary_address, private_account, None);
    vm.vm.run_validation(tx, 55)
}

const OUT_OF_GAS_CASES: [TestCase; 3] = [
    TestCase::PlainOutOfGas,
    TestCase::PlainOutOfGasWithCatch,
    TestCase::RecursiveOutOfGas,
];

pub(crate) fn test_validation_out_of_gas_with_full_tracer<VM: TestedVmForValidation>() {
    for test_case in OUT_OF_GAS_CASES {
        println!("Testing case: {test_case:?}");
        let (result, violated_rule) = test_rule::<VM>(300_000, test_case);
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
}

pub(crate) fn test_validation_out_of_gas_with_fast_tracer<VM: TestedVm>() {
    for test_case in OUT_OF_GAS_CASES {
        println!("Testing case: {test_case:?}");

        // Large tx gas limit should lead to a validation-specific halt reason.
        let tx_gas_limits: &[_] = if matches!(test_case, TestCase::RecursiveOutOfGas) {
            &[100_000_000, 200_000_000, 500_000_000, 1_000_000_000]
        } else {
            &[1_000_000, 10_000_000, 100_000_000, 1_000_000_000]
        };

        for &tx_gas_limit in tx_gas_limits {
            println!("Testing tx with gas limit: {tx_gas_limit}");
            let result = run_validation_with_gas_limit::<VM>(test_case, tx_gas_limit);
            assert_matches!(
                &result.result,
                ExecutionResult::Halt {
                    reason: Halt::ValidationOutOfGas,
                }
            );
        }

        // If the tx gas limit is lower than the validation gas limit, the bootloader should exit super-early.
        println!("Testing tx with low gas limit");
        let result = run_validation_with_gas_limit::<VM>(test_case, 250_000);
        assert_matches!(
            &result.result,
            ExecutionResult::Halt {
                reason: Halt::ValidationFailed(_)
            }
        );
    }
}

fn run_validation_with_gas_limit<VM: TestedVm>(
    test_case: TestCase,
    tx_gas_limit: u32,
) -> VmExecutionResultAndLogs {
    let aa_address = Address::repeat_byte(0x10);
    let beneficiary_address = Address::repeat_byte(0x20);
    let bytecode = TestContract::validation_test().bytecode.to_vec();

    // Configure the AA to run out of gas during validation.
    let mut storage_with_rule_break_set = get_empty_storage();
    storage_with_rule_break_set.set_value(
        StorageKey::new(AccountTreeId::new(aa_address), H256::zero()),
        H256::from_low_u64_be(test_case as u64),
    );

    let mut vm = VmTesterBuilder::new()
        .with_system_env(SystemEnv {
            default_validation_computational_gas_limit: 300_000,
            ..default_system_env()
        })
        .with_custom_contracts(vec![
            ContractToDeploy::account(bytecode, aa_address).funded()
        ])
        .with_storage(storage_with_rule_break_set)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let private_account = &mut vm.rich_accounts[0];
    let fee = Fee {
        gas_limit: tx_gas_limit.into(),
        ..Account::default_fee()
    };
    let tx = make_aa_transaction(aa_address, beneficiary_address, private_account, Some(fee));
    vm.vm.push_transaction(tx.into());
    vm.vm.execute(InspectExecutionMode::OneTx)
}
