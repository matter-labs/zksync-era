use ethabi::Token;
use zksync_eth_signer::TransactionParameters;
use zksync_test_contracts::{Account, TestContract};
use zksync_types::{
    fee::Fee, l2::L2Tx, transaction_request::TransactionRequest, Address, Eip712Domain, Execute,
    L2ChainId, Transaction, U256,
};

use super::{tester::VmTesterBuilder, ContractToDeploy, TestedVm};
use crate::interface::{InspectExecutionMode, TxExecutionMode, VmInterfaceExt};

/// This test deploys 'buggy' account abstraction code, and then tries accessing it both with legacy
/// and EIP712 transactions.
/// Currently we support both, but in the future, we should allow only EIP712 transactions to access the AA accounts.
pub(crate) fn test_require_eip712<VM: TestedVm>() {
    // Use 3 accounts:
    // - `private_address` - EOA account, where we have the key
    // - `account_address` - AA account, where the contract is deployed
    // - beneficiary - an EOA account, where we'll try to transfer the tokens.
    let aa_address = Address::repeat_byte(0x10);
    let beneficiary_address = Address::repeat_byte(0x20);

    let bytecode = TestContract::many_owners().bytecode.to_vec();
    let mut vm = VmTesterBuilder::new()
        .with_custom_contracts(vec![
            ContractToDeploy::account(bytecode, aa_address).funded()
        ])
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();
    assert_eq!(vm.get_eth_balance(beneficiary_address), U256::from(0));
    let mut private_account = vm.rich_accounts[0].clone();

    // First, let's set the owners of the AA account to the `private_address`.
    // (so that messages signed by `private_address`, are authorized to act on behalf of the AA account).
    let set_owners_function = TestContract::many_owners().function("setOwners");
    let encoded_input = set_owners_function
        .encode_input(&[Token::Array(vec![Token::Address(private_account.address)])])
        .unwrap();

    let tx = private_account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(aa_address),
            calldata: encoded_input,
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed());

    let private_account_balance = vm.get_eth_balance(private_account.address);

    // And now let's do the transfer from the 'account abstraction' to 'beneficiary' (using 'legacy' transaction).
    // Normally this would not work - unless the operator is malicious.
    let aa_raw_tx = TransactionParameters {
        nonce: U256::from(0),
        to: Some(beneficiary_address),
        gas: U256::from(100000000),
        gas_price: Some(U256::from(10000000)),
        value: U256::from(888000088),
        data: vec![],
        chain_id: 270,
        transaction_type: None,
        access_list: None,
        max_fee_per_gas: U256::from(1000000000),
        max_priority_fee_per_gas: U256::from(1000000000),
        max_fee_per_blob_gas: None,
        blob_versioned_hashes: None,
    };

    let aa_tx = private_account.sign_legacy_tx(aa_raw_tx);
    let (tx_request, hash) = TransactionRequest::from_bytes(&aa_tx, L2ChainId::from(270)).unwrap();

    let mut l2_tx: L2Tx = L2Tx::from_request(tx_request, 10000, false).unwrap();
    l2_tx.set_input(aa_tx, hash);
    // Pretend that operator is malicious and sets the initiator to the AA account.
    l2_tx.common_data.initiator_address = aa_address;
    let transaction: Transaction = l2_tx.into();

    vm.vm.push_transaction(transaction);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed());

    assert_eq!(
        vm.get_eth_balance(beneficiary_address),
        U256::from(888000088)
    );
    // Make sure that the tokens were transferred from the AA account.
    assert_eq!(
        private_account_balance,
        vm.get_eth_balance(private_account.address)
    );

    // Now send the 'classic' EIP712 transaction
    let transaction: Transaction =
        make_aa_transaction(aa_address, beneficiary_address, &mut private_account, None).into();
    vm.vm.push_transaction(transaction);
    vm.vm.execute(InspectExecutionMode::OneTx);

    assert_eq!(
        vm.get_eth_balance(beneficiary_address),
        U256::from(916375026)
    );
    assert_eq!(
        private_account_balance,
        vm.get_eth_balance(private_account.address)
    );
}

pub(crate) fn make_aa_transaction(
    aa_address: Address,
    beneficiary_address: Address,
    private_account: &mut Account,
    fee: Option<Fee>,
) -> L2Tx {
    let chain_id: u32 = 270;

    let nonce = private_account.nonce;
    private_account.nonce += 1;
    let tx_712 = L2Tx::new(
        Some(beneficiary_address),
        vec![],
        nonce,
        fee.unwrap_or_else(Account::default_fee),
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

    l2_tx
}
