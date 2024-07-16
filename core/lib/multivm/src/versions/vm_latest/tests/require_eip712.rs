use ethabi::Token;
use zksync_eth_signer::{EthereumSigner, TransactionParameters};
use zksync_system_constants::L2_BASE_TOKEN_ADDRESS;
use zksync_types::{
    fee::Fee, l2::L2Tx, transaction_request::TransactionRequest,
    utils::storage_key_for_standard_token_balance, AccountTreeId, Address, Eip712Domain, Execute,
    L2ChainId, Nonce, Transaction, U256,
};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        tests::{
            tester::{Account, VmTester, VmTesterBuilder},
            utils::read_many_owners_custom_account_contract,
        },
        HistoryDisabled,
    },
};

impl VmTester<HistoryDisabled> {
    pub(crate) fn get_eth_balance(&mut self, address: Address) -> U256 {
        let key = storage_key_for_standard_token_balance(
            AccountTreeId::new(L2_BASE_TOKEN_ADDRESS),
            &address,
        );
        self.vm.state.storage.storage.read_from_storage(&key)
    }
}

// TODO refactor this test it use too much internal details of the VM
#[tokio::test]
/// This test deploys 'buggy' account abstraction code, and then tries accessing it both with legacy
/// and EIP712 transactions.
/// Currently we support both, but in the future, we should allow only EIP712 transactions to access the AA accounts.
async fn test_require_eip712() {
    // Use 3 accounts:
    // - `private_address` - EOA account, where we have the key
    // - `account_address` - AA account, where the contract is deployed
    // - beneficiary - an EOA account, where we'll try to transfer the tokens.
    let account_abstraction = Account::random();
    let mut private_account = Account::random();
    let beneficiary = Account::random();

    let (bytecode, contract) = read_many_owners_custom_account_contract();
    let mut vm = VmTesterBuilder::new(HistoryDisabled)
        .with_empty_in_memory_storage()
        .with_custom_contracts(vec![(bytecode, account_abstraction.address, true)])
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(vec![account_abstraction.clone(), private_account.clone()])
        .build();

    assert_eq!(vm.get_eth_balance(beneficiary.address), U256::from(0));

    let chain_id: u32 = 270;

    // First, let's set the owners of the AA account to the `private_address`.
    // (so that messages signed by `private_address`, are authorized to act on behalf of the AA account).
    let set_owners_function = contract.function("setOwners").unwrap();
    let encoded_input = set_owners_function
        .encode_input(&[Token::Array(vec![Token::Address(private_account.address)])])
        .unwrap();

    let tx = private_account.get_l2_tx_for_execute(
        Execute {
            contract_address: account_abstraction.address,
            calldata: encoded_input,
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed());

    let private_account_balance = vm.get_eth_balance(private_account.address);

    // And now let's do the transfer from the 'account abstraction' to 'beneficiary' (using 'legacy' transaction).
    // Normally this would not work - unless the operator is malicious.
    let aa_raw_tx = TransactionParameters {
        nonce: U256::from(0),
        to: Some(beneficiary.address),
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

    let aa_tx = private_account.sign_legacy_tx(aa_raw_tx).await;
    let (tx_request, hash) = TransactionRequest::from_bytes(&aa_tx, L2ChainId::from(270)).unwrap();

    let mut l2_tx: L2Tx = L2Tx::from_request(tx_request, 10000).unwrap();
    l2_tx.set_input(aa_tx, hash);
    // Pretend that operator is malicious and sets the initiator to the AA account.
    l2_tx.common_data.initiator_address = account_abstraction.address;
    let transaction: Transaction = l2_tx.into();

    vm.vm.push_transaction(transaction);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed());

    assert_eq!(
        vm.get_eth_balance(beneficiary.address),
        U256::from(888000088)
    );
    // Make sure that the tokens were transferred from the AA account.
    assert_eq!(
        private_account_balance,
        vm.get_eth_balance(private_account.address)
    );

    // // Now send the 'classic' EIP712 transaction
    let tx_712 = L2Tx::new(
        beneficiary.address,
        vec![],
        Nonce(1),
        Fee {
            gas_limit: U256::from(1000000000),
            max_fee_per_gas: U256::from(1000000000),
            max_priority_fee_per_gas: U256::from(1000000000),
            gas_per_pubdata_limit: U256::from(1000000000),
        },
        account_abstraction.address,
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
        .await
        .unwrap();
    let encoded_tx = transaction_request.get_signed_bytes(&signature).unwrap();

    let (aa_txn_request, aa_hash) =
        TransactionRequest::from_bytes(&encoded_tx, L2ChainId::from(chain_id)).unwrap();

    let mut l2_tx = L2Tx::from_request(aa_txn_request, 100000).unwrap();
    l2_tx.set_input(encoded_tx, aa_hash);

    let transaction: Transaction = l2_tx.into();
    vm.vm.push_transaction(transaction);
    vm.vm.execute(VmExecutionMode::OneTx);

    assert_eq!(
        vm.get_eth_balance(beneficiary.address),
        U256::from(916375026)
    );
    assert_eq!(
        private_account_balance,
        vm.get_eth_balance(private_account.address)
    );
}
