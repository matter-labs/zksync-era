use once_cell::sync::Lazy;
use zksync_contracts::{read_zbin_bytecode, BaseSystemContracts, SystemContractCode};
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

pub(crate) fn read_max_depth_contract() -> Vec<u8> {
    read_zbin_bytecode(
        "core/tests/ts-integration/contracts/zkasm/artifacts/deep_stak.zkasm/deep_stak.zkasm.zbin",
    )
}
