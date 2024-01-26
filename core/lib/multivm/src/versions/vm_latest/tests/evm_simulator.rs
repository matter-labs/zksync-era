use std::str::FromStr;

use ethabi::Contract;
use zk_evm_1_5_0::zkevm_opcode_defs::{BlobSha256Format, VersionedHashLen32};
use zksync_contracts::BaseSystemContracts;
use zksync_state::InMemoryStorage;
use zksync_system_constants::L2_ETH_TOKEN_ADDRESS;
use zksync_types::{
    get_code_key, get_known_code_key, get_nonce_key,
    system_contracts::{DEPLOYMENT_NONCE_INCREMENT, TX_NONCE_INCREMENT},
    AccountTreeId, Address, Execute, StorageKey, H256, U256,
};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256, u256_to_h256};

use super::tester::VmTester;
use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        tests::{
            tester::{DeployContractsTx, TxType, VmTesterBuilder},
            utils::{
                get_balance, read_test_contract, read_test_evm_simulator, verify_required_storage,
            },
        },
        utils::fee::get_batch_base_fee,
        HistoryEnabled,
    },
    vm_m5::storage::Storage,
};

const EXPECTED_EVM_STIPEND: u32 = (1 << 30);

fn set_up_evm_simulator_contract() -> (VmTester<HistoryEnabled>, Address, Contract) {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);

    // Just some address in user space
    let test_address = Address::from_str("0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7").unwrap();

    // We don't even need the preimage of it, just need the correct format to trigger the simulator
    let sample_evm_bytecode_hash = {
        let mut hash = [0u8; 32];
        hash[0] = BlobSha256Format::VERSION_BYTE;
        assert!(BlobSha256Format::is_valid(&hash));
        H256(hash)
    };

    storage.set_value(get_code_key(&test_address), sample_evm_bytecode_hash);

    let (evm_simulator_bytecode, evm_simualator) = read_test_evm_simulator();

    let mut base_system_contracts = BaseSystemContracts::playground();
    base_system_contracts.evm_simualator = hash_bytecode(&evm_simulator_bytecode);

    let vm: crate::vm_latest::tests::tester::VmTester<HistoryEnabled> =
        VmTesterBuilder::new(HistoryEnabled)
            .with_storage(storage)
            .with_base_system_smart_contracts(base_system_smart_contracts)
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_random_rich_accounts(1)
            .build();

    (vm, test_address, evm_simualator)
}

#[test]
fn test_evm_simulator_get_gas() {
    // We check that simulator receives the stipend
    let (mut vm, test_address, evm_simulator) = set_up_evm_simulator_contract();

    let account = &mut vm.rich_accounts[0];

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_address),
            calldata: evm_simulator
                .function("getGas")
                .unwrap()
                .encode_input(&[])
                .unwrap(),
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );

    vm.vm.push_transaction(tx);
    let tx_result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    let batch_result = vm.vm.execute(VmExecutionMode::Batch);
    assert!(!batch_result.result.is_failed(), "Batch wasn't successful");

    let saved_value = vm.vm.storage.borrow_mut().get_value(&StorageKey::new(
        AccountTreeId::new(test_address),
        H256::zero(),
    ));
    let gas_received = h256_to_u256(saved_value).as_u32();

    assert!(gas_received > EXPECTED_EVM_STIPEND, "Stipend wasnt applied");
}
