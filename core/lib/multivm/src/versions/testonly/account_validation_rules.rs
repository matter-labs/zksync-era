use zksync_types::{
    fee::Fee, l2::L2Tx, transaction_request::TransactionRequest, Address, Eip712Domain, L2ChainId,
    Transaction, U256,
};
use zksync_vm_interface::{ExecutionResult, Halt, VmRevertReason};

use super::{read_validation_test_contract, tester::VmTesterBuilder, ContractToDeploy, TestedVm};
use crate::interface::{TxExecutionMode, VmExecutionMode, VmInterfaceExt};

/// Checks that every limitation imposed on account validation results in an appropriate error.
pub(crate) fn test_account_validation_rules<VM: TestedVm>() {
    let aa_address = Address::repeat_byte(0x10);
    let beneficiary_address = Address::repeat_byte(0x20);

    let bytecode = read_validation_test_contract();
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_custom_contracts(vec![
            ContractToDeploy::account(bytecode, aa_address).funded()
        ])
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let chain_id: u32 = 270;
    let private_account = vm.rich_accounts[0].clone();

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

    let transaction: Transaction = l2_tx.into();
    vm.vm.push_transaction(transaction);
    let result = vm.vm.execute(VmExecutionMode::OneTx);

    assert!(result.result.is_failed());
    match result.result {
        ExecutionResult::Halt {
            reason:
                Halt::ValidationFailed(VmRevertReason::Unknown {
                    function_selector: s,
                    data,
                }),
        } => {
            s.into_iter().for_each(|x| print!("{:x}", x));
            println!();
            dbg!(data);
        }
        _ => {}
    }
}
