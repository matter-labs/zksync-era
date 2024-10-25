use assert_matches::assert_matches;
use ethabi::Token;
use zksync_types::{
    fee::Fee, l2::L2Tx, transaction_request::TransactionRequest, Address, Eip712Domain, Execute,
    L2ChainId, U256,
};
use zksync_vm_interface::{tracer::ViolatedValidationRule, InspectExecutionMode, VmInterfaceExt};

use super::{
    read_validation_test_contract, tester::VmTesterBuilder, ContractToDeploy, TestedVm,
    TestedVmForValidation,
};
use crate::interface::TxExecutionMode;

/// Checks that every limitation imposed on account validation results in an appropriate error.
/// The actual misbehaviours are found in "validation-rule-breaker.sol".
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

    // Disallowing gasleft is difficult because it isn't immediately followed
    // by far call in EraVM bytecode.
    /*assert_matches!(
        test_rule::<VM>(100),
        Some(ViolatedValidationRule::TouchedDisallowedContext)
    );*/

    // TODO: test running out of gas but catching the failure.
    // Can be accomplished via many nested far calls.
}

fn test_rule<VM: TestedVm + TestedVmForValidation>(rule: u32) -> Option<ViolatedValidationRule> {
    let aa_address = Address::repeat_byte(0x10);
    let beneficiary_address = Address::repeat_byte(0x20);

    let (bytecode, contract) = read_validation_test_contract();
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_custom_contracts(vec![
            ContractToDeploy::account(bytecode, aa_address).funded()
        ])
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let mut private_account = vm.rich_accounts[0].clone();

    // Set the type of misbehaviour of the AA contract
    let function = contract.function("setTypeOfRuleBreak").unwrap();
    let transaction = private_account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(aa_address),
            calldata: function.encode_input(&[Token::Uint(rule.into())]).unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(transaction);
    assert!(!vm
        .vm
        .execute(InspectExecutionMode::OneTx)
        .result
        .is_failed());

    // Use account abstraction
    let chain_id: u32 = 270;

    let tx_712 = L2Tx::new(
        Some(beneficiary_address),
        vec![],
        private_account.nonce,
        Fee {
            gas_limit: U256::from(1000000000),
            max_fee_per_gas: U256::from(1000000000),
            max_priority_fee_per_gas: U256::from(1000000000),
            gas_per_pubdata_limit: U256::from(1000000000),
        },
        aa_address,
        U256::from(28374938),
        vec![],
        Default::default(),
    );

    let mut transaction_request: TransactionRequest = tx_712.into();
    transaction_request.chain_id = Some(chain_id.into());

    let domain = Eip712Domain::new(L2ChainId::from(chain_id));
    let signature = private_account
        .get_pk_signer()
        .sign_typed_data(&domain, &transaction_request)
        .unwrap();
    let encoded_tx = transaction_request.get_signed_bytes(&signature).unwrap();

    let (aa_txn_request, aa_hash) =
        TransactionRequest::from_bytes(&encoded_tx, L2ChainId::from(chain_id)).unwrap();

    let mut l2_tx = L2Tx::from_request(aa_txn_request, 100000, false).unwrap();
    l2_tx.set_input(encoded_tx, aa_hash);

    vm.vm.run_validation(l2_tx)
}
