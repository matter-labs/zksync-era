use ethabi::Contract;
use once_cell::sync::Lazy;
// FIXME: use 1.5.0 once available
use zk_evm_1_4_1::sha2;
use zk_evm_1_5_0::zkevm_opcode_defs::{BlobSha256Format, VersionedHashLen32};
use zksync_contracts::{
    load_contract, read_bytecode, read_evm_bytecode, read_zbin_bytecode, BaseSystemContracts,
    SystemContractCode,
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{
    utils::storage_key_for_standard_token_balance, web3::signing::keccak256, AccountTreeId,
    Address, StorageKey, H256, U256,
};
use zksync_utils::{
    address_to_h256, bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256,
};

use crate::vm_latest::{
    tests::tester::InMemoryStorageView, types::internals::ZkSyncVmState, HistoryMode,
};

pub(crate) static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

// Probably make it a part of vm tester
pub(crate) fn verify_required_storage<H: HistoryMode>(
    state: &ZkSyncVmState<InMemoryStorageView, H>,
    required_values: Vec<(H256, StorageKey)>,
) {
    for (required_value, key) in required_values {
        let current_value = state.storage.storage.read_from_storage(&key);

        assert_eq!(
            u256_to_h256(current_value),
            required_value,
            "Invalid value at key {key:?}"
        );
    }
}

pub(crate) fn verify_required_memory<H: HistoryMode>(
    state: &ZkSyncVmState<InMemoryStorageView, H>,
    required_values: Vec<(U256, u32, u32)>,
) {
    for (required_value, memory_page, cell) in required_values {
        let current_value = state
            .memory
            .read_slot(memory_page as usize, cell as usize)
            .value;
        assert_eq!(current_value, required_value);
    }
}

pub(crate) fn get_balance<S: WriteStorage>(
    token_id: AccountTreeId,
    account: &Address,
    main_storage: StoragePtr<S>,
) -> U256 {
    let key = storage_key_for_standard_token_balance(token_id, account);
    h256_to_u256(main_storage.borrow_mut().read_value(&key))
}

pub(crate) fn read_test_contract() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json")
}

pub(crate) fn get_bootloader(test: &str) -> SystemContractCode {
    let bootloader_code = read_zbin_bytecode(format!(
        "contracts/system-contracts/bootloader/tests/artifacts/{}.yul.zbin",
        test
    ));

    let bootloader_hash = hash_bytecode(&bootloader_code);
    SystemContractCode {
        code: bytes_to_be_words(bootloader_code),
        hash: bootloader_hash,
    }
}

pub(crate) fn read_nonce_holder_tester() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/custom-account/nonce-holder-test.sol/NonceHolderTest.json")
}

pub(crate) fn read_error_contract() -> Vec<u8> {
    read_bytecode(
        "etc/contracts-test-data/artifacts-zk/contracts/error/error.sol/SimpleRequire.json",
    )
}

pub(crate) fn get_execute_error_calldata() -> Vec<u8> {
    let test_contract = load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/error/error.sol/SimpleRequire.json",
    );

    let function = test_contract.function("require_short").unwrap();

    function
        .encode_input(&[])
        .expect("failed to encode parameters")
}

pub(crate) fn read_many_owners_custom_account_contract() -> (Vec<u8>, Contract) {
    let path = "etc/contracts-test-data/artifacts-zk/contracts/custom-account/many-owners-custom-account.sol/ManyOwnersCustomAccount.json";
    (read_bytecode(path), load_contract(path))
}

pub(crate) fn read_test_evm_simulator() -> (Vec<u8>, Contract) {
    let path = "etc/contracts-test-data/artifacts-zk/contracts/evm-simulator/EvmSimulatorTest.sol/EvmSimulatorTest.json";
    (read_bytecode(path), load_contract(path))
}

pub(crate) fn read_erc20_contract() -> (Vec<u8>, Contract) {
    let path = "etc/ERC20/artifacts-zk/contracts/ZkSyncERC20.sol/ZkSyncERC20.json";
    (read_bytecode(path), load_contract(path))
}

pub(crate) fn read_max_depth_contract() -> Vec<u8> {
    read_zbin_bytecode(
        "core/tests/ts-integration/contracts/zkasm/artifacts/deep_stak.zkasm/deep_stak.zkasm.zbin",
    )
}

pub(crate) fn read_precompiles_contract() -> Vec<u8> {
    read_bytecode(
        "etc/contracts-test-data/artifacts-zk/contracts/precompiles/precompiles.sol/Precompiles.json",
    )
}

pub(crate) fn load_precompiles_contract() -> Contract {
    load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/precompiles/precompiles.sol/Precompiles.json",
    )
}

pub(crate) fn key_for_evm_hash(address: &Address) -> H256 {
    let address_h256 = address_to_h256(address);

    let bytes = [address_h256.as_bytes(), u256_to_h256(1.into()).as_bytes()].concat();
    keccak256(&bytes).into()
}

pub(crate) fn read_complex_upgrade() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/complex-upgrade.sol/ComplexUpgrade.json")
}

pub(crate) fn get_complex_upgrade_abi() -> Contract {
    load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/complex-upgrade.sol/ComplexUpgrade.json"
    )
}

pub(crate) fn read_test_evm_bytecode(folder_name: &str, contract_name: &str) -> (Vec<u8>, Vec<u8>) {
    let contract_lowercase = contract_name.to_lowercase();
    read_evm_bytecode(format!("etc/evm-contracts-test-data/artifacts/contracts/{folder_name}/{contract_lowercase}.sol/{contract_name}.json"))
}

pub(crate) fn load_test_evm_contract(folder_name: &str, contract_name: &str) -> Contract {
    let contract_lowercase = contract_name.to_lowercase();
    load_contract(format!(
        "etc/evm-contracts-test-data/artifacts/contracts/{folder_name}/{contract_lowercase}.sol/{contract_name}.json",
    ))
}
