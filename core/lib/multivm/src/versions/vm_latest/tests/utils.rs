use ethabi::{Contract, Token};
use once_cell::sync::Lazy;
use zksync_basic_types::H160;
use zksync_contracts::{
    load_contract, read_bytecode, read_zbin_bytecode, BaseSystemContracts, SystemContractCode,
};
use zksync_test_account::Account;
use zksync_types::{
    get_code_key, get_known_code_key, utils::storage_key_for_standard_token_balance, AccountTreeId,
    Address, Execute, StorageKey, H256, U256,
};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256};

use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        ExecutionResult, VmExecutionMode, VmInterface,
    },
    vm_latest::{
        tests::tester::{InMemoryStorageView, TxType, VmTester},
        types::internals::ZkSyncVmState,
        HistoryEnabled, HistoryMode,
    },
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

pub(crate) fn read_admin_facet() -> (Vec<u8>, Contract) {
    let path = "contracts/l1-contracts/artifacts-zk/contracts/state-transition/chain-deps/facets/Admin.sol/AdminFacet.json";
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

pub(crate) fn read_diamond_proxy() -> (Vec<u8>, Contract) {
    let path = "contracts/l1-contracts/artifacts-zk/contracts/state-transition/chain-deps/DiamondProxy.sol/DiamondProxy.json";
    (read_bytecode(path), load_contract(path))
}

pub(crate) fn read_diamond() -> (Vec<u8>, Contract) {
    let path = "contracts/l1-contracts/artifacts-zk/contracts/state-transition/libraries/Diamond.sol/Diamond.json";
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

fn sub_h160(lhs: H160, rhs: H160) -> H160 {
    // Convert both H160 values to U256
    let lhs_u256 = U256::from_big_endian(&lhs.0);
    let rhs_u256 = U256::from_big_endian(&rhs.0);

    // Calculate 2^160 as U256
    let two_to_160 = U256::from(1) << 160;

    // Add 2^160 to lhs to ensure no underflow, then subtract rhs
    let result_u256 = (lhs_u256 + two_to_160 - rhs_u256) % two_to_160;

    // Create an array for the full 32-byte U256 representation
    let mut u256_bytes = [0u8; 32];

    // Convert the U256 to a 32-byte array (big-endian)
    result_u256.to_big_endian(&mut u256_bytes);

    // Now, extract the last 20 bytes from this 32-byte array for H160
    let mut result_h160 = [0u8; 20];
    result_h160.copy_from_slice(&u256_bytes[12..]);

    H160(result_h160)
}

pub fn undo_l1_to_l2_alias(l2_address: H160) -> H160 {
    const OFFSET: H160 = H160([
        0x11, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x11, 0x11,
    ]);
    sub_h160(l2_address, OFFSET)
}

pub fn send_prank_tx_and_verify(
    vm: &mut VmTester<HistoryEnabled>,
    contract_address: Address,
    factory_deps: Vec<Vec<u8>>,
    calldata: Vec<u8>,
    prank_address: Address,
) {
    let account: &mut Account = &mut vm.rich_accounts[0];

    let tx = account.get_l1_tx_custom_sender(
        Execute {
            contract_address,
            value: U256::zero(),
            factory_deps,
            calldata,
        },
        0,
        prank_address,
    );
    vm.vm.push_transaction(tx);

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    match res.result {
        ExecutionResult::Success { output } => {
            println!("Transaction was successful. Output: {:?}", output);
        }
        ExecutionResult::Revert { output } => {
            eprintln!("Transaction reverted. Reason: {:?}", output);
            panic!("Transaction wasn't successful: {:?}", output);
        }
        ExecutionResult::Halt { reason } => {
            eprintln!("Transaction halted. Reason: {:?}", reason);
            panic!("Transaction wasn't successful: {:?}", reason);
        }
    }
}

pub fn deploy_and_verify_contract(
    vm: &mut VmTester<HistoryEnabled>,
    contract_code: &Vec<u8>,
    constructor_data: Option<&[Token]>,
    deploy_nonce: &mut u64,
) -> Address {
    let account: &mut Account = &mut vm.rich_accounts[0];

    let deploy_tx = account.get_deploy_tx(
        contract_code,
        constructor_data,
        TxType::L1 {
            serial_id: *deploy_nonce,
        },
    );
    *deploy_nonce += 1;
    let contract_address = deploy_tx.address.clone();

    vm.vm.push_transaction(deploy_tx.tx.clone());

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    // The code hash of the deployed contract should be marked as republished.
    let known_codes_key = get_known_code_key(&deploy_tx.bytecode_hash);

    // The contract should be deployed successfully.
    let account_code_key = get_code_key(&deploy_tx.address);

    let expected_slots = vec![
        (u256_to_h256(U256::from(1u32)), known_codes_key),
        (deploy_tx.bytecode_hash, account_code_key),
    ];
    assert!(!res.result.is_failed());

    verify_required_storage(&vm.vm.state, expected_slots);

    contract_address
}

pub fn send_l2_tx_and_verify(
    vm: &mut VmTester<HistoryEnabled>,
    contract_address: Address,
    calldata: Vec<u8>,
) {
    let account: &mut Account = &mut vm.rich_accounts[0];

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address,
            calldata,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    assert!(!res.result.is_failed());
}
