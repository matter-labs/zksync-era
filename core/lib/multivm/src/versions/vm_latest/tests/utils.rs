use ethabi::Contract;
use once_cell::sync::Lazy;
use zksync_contracts::{
    load_contract, read_bytecode, read_zbin_bytecode, BaseSystemContracts, SystemContractCode,
};
use zksync_types::{
    utils::storage_key_for_standard_token_balance, AccountTreeId, Address, StorageKey, H256, U256,
};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256};

use crate::{
    interface::storage::{StoragePtr, WriteStorage},
    vm_latest::{tests::tester::InMemoryStorageView, types::internals::ZkSyncVmState, HistoryMode},
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

pub(crate) fn read_message_root() -> Vec<u8> {
    read_bytecode(
        "contracts/l1-contracts/artifacts-zk/contracts/bridgehub/MessageRoot.sol/MessageRoot.json",
    )
}

pub(crate) fn read_bridgehub() -> (Vec<u8>, Contract) {
    let path =
        "contracts/l1-contracts/artifacts-zk/contracts/bridgehub/Bridgehub.sol/Bridgehub.json";
    (read_bytecode(path), load_contract(path))
}

pub(crate) fn read_stm() -> (Vec<u8>, Contract) {
    let path = "contracts/l1-contracts/artifacts-zk/contracts/state-transition/StateTransitionManager.sol/StateTransitionManager.json";
    (read_bytecode(path), load_contract(path))
}

pub(crate) fn read_mailbox_facet() -> (Vec<u8>, Contract) {
    let path = "contracts/l1-contracts/artifacts-zk/contracts/state-transition/chain-deps/facets/Mailbox.sol/MailboxFacet.json";
    (read_bytecode(path), load_contract(path))
}

pub(crate) fn read_diamond_init() -> (Vec<u8>, Contract) {
    let path = "contracts/l1-contracts/artifacts-zk/contracts/state-transition/chain-deps/DiamondInit.sol/DiamondInit.json";
    (read_bytecode(path), load_contract(path))
}

pub(crate) fn read_transparent_proxy() -> (Vec<u8>, Contract) {
    let path = "contracts/l1-contracts/artifacts-zk/@openzeppelin/contracts/proxy/transparent/TransparentUpgradeableProxy.sol/TransparentUpgradeableProxy.json";
    (read_bytecode(path), load_contract(path))
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

pub(crate) fn read_expensive_contract() -> (Vec<u8>, Contract) {
    const PATH: &str =
        "etc/contracts-test-data/artifacts-zk/contracts/expensive/expensive.sol/Expensive.json";
    (read_bytecode(PATH), load_contract(PATH))
}

pub(crate) fn read_mailbox_contract() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/mailbox/mailbox.sol/Mailbox.json")
}
