use std::collections::HashMap;

use era_vm::{store::StorageKey, Execution};
use ethabi::Contract;
use once_cell::sync::Lazy;
use vm2::HeapId;
use zksync_contracts::{
    load_contract, read_bytecode, read_zbin_bytecode, BaseSystemContracts, SystemContractCode,
};
use zksync_state::ReadStorage;
use zksync_types::{
    self, utils::storage_key_for_standard_token_balance, AccountTreeId, Address,
    StorageKey as ZKStorageKey, H256, U256,
};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256};

use crate::vm_latest::CurrentExecutionState;

pub(crate) static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

pub fn lambda_storage_key_to_zk(key: StorageKey) -> ZKStorageKey {
    ZKStorageKey::new(AccountTreeId::new(key.address), u256_to_h256(key.key))
}

pub fn zk_storage_key_to_lambda(key: &ZKStorageKey) -> StorageKey {
    StorageKey {
        address: key.address().clone(),
        key: h256_to_u256(key.key().clone()),
    }
}

pub fn sort_execution_state(state: &mut CurrentExecutionState) {
    // sort events
    state.events.sort_by(|a, b| a.address.cmp(&b.address));

    // sort user_l2_to_l1_logs
    state.user_l2_to_l1_logs.sort_by(|a, b| {
        a.0.key
            .cmp(&b.0.key)
            .then_with(|| a.0.sender.cmp(&b.0.sender))
    });

    // sort system_logs
    state.system_logs.sort_by(|a, b| {
        a.0.key
            .cmp(&b.0.key)
            .then_with(|| a.0.sender.cmp(&b.0.sender))
    });

    // sort deduplicated_storage_logs
    state.deduplicated_storage_logs.sort_by(|a, b| {
        a.key
            .account()
            .address()
            .cmp(&b.key.account().address())
            .then_with(|| a.key.key().cmp(b.key.key()))
    });

    // sort used_contract_hashes
    state.used_contract_hashes.sort();
}

pub(crate) fn verify_required_memory(vm: &Execution, required_values: Vec<(U256, HeapId, u32)>) {
    for (required_value, memory_page, cell) in required_values {
        let current_value = vm.heaps.get(memory_page.to_u32()).unwrap().read(cell * 32);
        assert_eq!(current_value, required_value);
    }
}

pub(crate) fn verify_required_storage(
    required_values: &[(H256, ZKStorageKey)],
    main_storage: &mut impl ReadStorage,
    storage_changes: &HashMap<StorageKey, U256>,
) {
    for &(required_value, key) in required_values {
        let current_value = storage_changes
            .get(&zk_storage_key_to_lambda(&key))
            .copied()
            .unwrap_or_else(|| h256_to_u256(main_storage.read_value(&key)));

        assert_eq!(
            u256_to_h256(current_value),
            required_value,
            "Invalid value at key {key:?}"
        );
    }
}
pub(crate) fn get_balance(
    token_id: AccountTreeId,
    account: &Address,
    main_storage: &mut impl ReadStorage,
    storage_changes: &HashMap<StorageKey, U256>,
) -> U256 {
    let key = storage_key_for_standard_token_balance(token_id, account);

    storage_changes
        .get(&zk_storage_key_to_lambda(&key))
        .copied()
        .unwrap_or_else(|| h256_to_u256(main_storage.read_value(&key)))
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

pub(crate) fn read_nonce_holder_tester() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/custom-account/nonce-holder-test.sol/NonceHolderTest.json")
}

pub(crate) fn read_complex_upgrade() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/complex-upgrade.sol/ComplexUpgrade.json")
}

pub(crate) fn get_complex_upgrade_abi() -> Contract {
    load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/complex-upgrade.sol/ComplexUpgrade.json"
    )
}

pub(crate) fn read_expensive_contract() -> (Vec<u8>, Contract) {
    const PATH: &str =
        "etc/contracts-test-data/artifacts-zk/contracts/expensive/expensive.sol/Expensive.json";
    (read_bytecode(PATH), load_contract(PATH))
}
