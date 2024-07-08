use std::str::FromStr;

use ethabi::Contract;
use once_cell::sync::Lazy;
use zksync_contracts::{
    load_contract, read_bytecode, read_zbin_bytecode, BaseSystemContracts, SystemContractCode,
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{
    utils::storage_key_for_standard_token_balance, AccountTreeId, Address, StorageKey, H256,
    L2_MESSAGE_ROOT_ADDRESS, U256,
};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256};

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

pub(crate) fn read_simple_transfer_contract() -> Vec<u8> {
    read_bytecode(
        "etc/contracts-test-data/artifacts-zk/contracts/simple-transfer/simple-transfer.sol/SimpleTransfer.json",
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

pub(crate) fn read_complex_upgrade() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/complex-upgrade.sol/ComplexUpgrade.json")
}

pub(crate) fn get_complex_upgrade_abi() -> Contract {
    load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/complex-upgrade.sol/ComplexUpgrade.json"
    )
}

pub(crate) fn initialize_message_root_storage(storage: StoragePtr<InMemoryStorageView>) {
    let account = AccountTreeId::new(L2_MESSAGE_ROOT_ADDRESS);
    let mut storage_ptr = storage.as_ref().borrow_mut();
    let initialized_slots = vec![
        (
            "8e94fed44239eb2314ab7a406345e6c5a8f0ccedf3b600de3d004e672c33abf4",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ),
        (
            "0000000000000000000000000000000000000000000000000000000000000007",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ),
        (
            "a66cc928b5edb82af9bd49922954155ab7b0942694bea4ce44661d9a8736c688",
            "46700b4d40ac5c35af2c22dda2787a91eb567b06c924a8fb8ae9a05b20c08c21",
        ),
        (
            "0000000000000000000000000000000000000000000000000000000000000006",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ),
        (
            "f652222313e28459528d920b65115c16c04f3efc82aaedc97be59f3f377c0d3f",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ),
        (
            "b868bdfa8727775661e4ccf117824a175a33f8703d728c04488fbfffcafda9f9",
            "46700b4d40ac5c35af2c22dda2787a91eb567b06c924a8fb8ae9a05b20c08c21",
        ),
    ];

    initialized_slots.iter().for_each(|(k, v)| {
        let key = H256::from_str(k).unwrap();
        let value = H256::from_str(v).unwrap();

        let storage_key = StorageKey::new(account, key);
        storage_ptr.set_value(storage_key, value);
    });
}
