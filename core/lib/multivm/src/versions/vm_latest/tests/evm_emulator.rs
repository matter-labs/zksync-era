use ethabi::Token;
use zksync_contracts::read_bytecode;
use zksync_system_constants::{CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS};
use zksync_types::{get_code_key, get_known_code_key, Execute, H256};
use zksync_utils::{be_words_to_bytes, bytecode::hash_bytecode, h256_to_u256};
use zksync_vm_interface::VmInterfaceExt;

use crate::{
    interface::{storage::InMemoryStorage, TxExecutionMode},
    versions::testonly::default_system_env,
    vm_latest::{tests::tester::VmTesterBuilder, utils::hash_evm_bytecode, HistoryEnabled},
};

const MOCK_DEPLOYER_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockContractDeployer.json";
const MOCK_KNOWN_CODE_STORAGE_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockKnownCodeStorage.json";

#[test]
fn tracing_evm_contract_deployment() {
    let mock_deployer = read_bytecode(MOCK_DEPLOYER_PATH);
    let mock_deployer_hash = hash_bytecode(&mock_deployer);
    let mock_known_code_storage = read_bytecode(MOCK_KNOWN_CODE_STORAGE_PATH);
    let mock_known_code_storage_hash = hash_bytecode(&mock_known_code_storage);

    // Override
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    storage.set_value(get_code_key(&CONTRACT_DEPLOYER_ADDRESS), mock_deployer_hash);
    storage.set_value(
        get_known_code_key(&mock_deployer_hash),
        H256::from_low_u64_be(1),
    );
    storage.set_value(
        get_code_key(&KNOWN_CODES_STORAGE_ADDRESS),
        mock_known_code_storage_hash,
    );
    storage.set_value(
        get_known_code_key(&mock_known_code_storage_hash),
        H256::from_low_u64_be(1),
    );
    storage.store_factory_dep(mock_deployer_hash, mock_deployer);
    storage.store_factory_dep(mock_known_code_storage_hash, mock_known_code_storage);

    let mut system_env = default_system_env();
    // The EVM emulator will not be accessed, so we set it to a dummy value.
    system_env.base_system_smart_contracts.evm_emulator =
        Some(system_env.base_system_smart_contracts.default_aa.clone());
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_system_env(system_env)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();
    let account = &mut vm.rich_accounts[0];

    let args = [Token::Bytes((0..=u8::MAX).collect())];
    let evm_bytecode = ethabi::encode(&args);
    let expected_bytecode_hash = hash_evm_bytecode(&evm_bytecode);
    let execute = Execute::for_deploy(expected_bytecode_hash, vec![0; 32], &args);
    let deploy_tx = account.get_l2_tx_for_execute(execute, None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    // Check that the surrogate EVM bytecode was added to the decommitter.
    let known_bytecodes = vm.vm.state.decommittment_processor.known_bytecodes.inner();
    let known_evm_bytecode =
        be_words_to_bytes(&known_bytecodes[&h256_to_u256(expected_bytecode_hash)]);
    assert_eq!(known_evm_bytecode, evm_bytecode);

    let new_known_factory_deps = vm_result.new_known_factory_deps.unwrap();
    assert_eq!(new_known_factory_deps.len(), 2); // the deployed EraVM contract + EVM contract
    assert_eq!(
        new_known_factory_deps[&expected_bytecode_hash],
        evm_bytecode
    );
}
