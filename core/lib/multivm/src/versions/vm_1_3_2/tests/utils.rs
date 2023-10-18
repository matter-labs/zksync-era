// //!
// //! Tests for the bootloader
// //! The description for each of the tests can be found in the corresponding `.yul` file.
// //!
// use zksync_types::{
//     ethabi::Contract,
//     Execute, L1TxCommonData, H160, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE,
//     {ethabi::Token, Address, ExecuteTransactionCommon, Transaction, U256},
// };

// use zksync_contracts::{load_contract, read_bytecode};
// use zksync_state::{InMemoryStorage, StorageView};
// use zksync_utils::bytecode::hash_bytecode;

// use crate::test_utils::get_create_execute;

// pub fn read_test_contract() -> Vec<u8> {
//     read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json")
// }

// pub fn read_long_return_data_contract() -> Vec<u8> {
//     read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/long-return-data/long-return-data.sol/LongReturnData.json")
// }

// pub fn read_nonce_holder_tester() -> Vec<u8> {
//     read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/custom-account/nonce-holder-test.sol/NonceHolderTest.json")
// }

// pub fn read_error_contract() -> Vec<u8> {
//     read_bytecode(
//         "etc/contracts-test-data/artifacts-zk/contracts/error/error.sol/SimpleRequire.json",
//     )
// }

// pub fn read_many_owners_custom_account_contract() -> (Vec<u8>, Contract) {
//     let path = "etc/contracts-test-data/artifacts-zk/contracts/custom-account/many-owners-custom-account.sol/ManyOwnersCustomAccount.json";
//     (read_bytecode(path), load_contract(path))
// }

// pub fn get_l1_execute_test_contract_tx_with_sender(
//     sender: Address,
//     deployed_address: Address,
//     with_panic: bool,
//     value: U256,
//     payable: bool,
// ) -> Transaction {
//     let execute = execute_test_contract(deployed_address, with_panic, value, payable);

//     Transaction {
//         common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
//             sender,
//             gas_limit: U256::from(200_000_000u32),
//             gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
//             to_mint: value,
//             ..Default::default()
//         }),
//         execute,
//         received_timestamp_ms: 0,
//     }
// }

// fn execute_test_contract(
//     address: Address,
//     with_panic: bool,
//     value: U256,
//     payable: bool,
// ) -> Execute {
//     let test_contract = load_contract(
//         "etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json",
//     );

//     let function = if payable {
//         test_contract
//             .function("incrementWithRevertPayable")
//             .unwrap()
//     } else {
//         test_contract.function("incrementWithRevert").unwrap()
//     };

//     let calldata = function
//         .encode_input(&[Token::Uint(U256::from(1u8)), Token::Bool(with_panic)])
//         .expect("failed to encode parameters");

//     Execute {
//         contract_address: address,
//         calldata,
//         value,
//         factory_deps: None,
//     }
// }

// pub fn get_l1_deploy_tx(code: &[u8], calldata: &[u8]) -> Transaction {
//     let execute = get_create_execute(code, calldata);

//     Transaction {
//         common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
//             sender: H160::random(),
//             gas_limit: U256::from(2000000u32),
//             gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
//             ..Default::default()
//         }),
//         execute,
//         received_timestamp_ms: 0,
//     }
// }

// pub fn create_storage_view() -> StorageView<InMemoryStorage> {
//     let raw_storage = InMemoryStorage::with_system_contracts(hash_bytecode);
//     StorageView::new(raw_storage)
// }
