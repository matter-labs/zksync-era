use ethabi::Contract;
use once_cell::sync::Lazy;
use vm2::{instruction_handlers::HeapInterface, HeapId, State, WorldDiff};
use zksync_contracts::{
    load_contract, read_bytecode, read_zbin_bytecode, BaseSystemContracts, SystemContractCode,
};
use zksync_state::ReadStorage;
use zksync_types::{utils::storage_key_for_standard_token_balance, AccountTreeId, Address, U256};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256};

pub(crate) static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

pub(crate) fn verify_required_memory(state: &State, required_values: Vec<(U256, HeapId, u32)>) {
    for (required_value, memory_page, cell) in required_values {
        let current_value = state.heaps[memory_page].read_u256(cell * 32);
        assert_eq!(current_value, required_value);
    }
}

pub(crate) fn get_balance(
    token_id: AccountTreeId,
    account: &Address,
    main_storage: &mut impl ReadStorage,
    diff: &WorldDiff,
) -> U256 {
    let key = storage_key_for_standard_token_balance(token_id, account);

    diff.storage_slot(*key.account().address(), h256_to_u256(*key.key()))
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
