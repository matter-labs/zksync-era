use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use itertools::Itertools;
use zk_evm_1_5_0::{
    abstractions::DecommittmentProcessor,
    aux_structures::{DecommittmentQuery, MemoryPage, Timestamp},
    zkevm_opcode_defs::{VersionedHashHeader, VersionedHashNormalizedPreimage},
};
use zksync_state::WriteStorage;
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_test_account::Account;
use zksync_types::{Execute, U256};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        tests::{
            tester::{TxType, VmTesterBuilder},
            utils::{read_test_contract, BASE_SYSTEM_CONTRACTS},
        },
        HistoryDisabled, Vm,
    },
    HistoryMode,
};

#[test]
fn test_get_used_contracts() {
    let mut vm = VmTesterBuilder::new(HistoryDisabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    assert!(known_bytecodes_without_aa_code(&vm.vm).is_empty());

    // create and push and execute some not-empty factory deps transaction with success status
    // to check that `get_used_contracts()` updates
    let contract_code = read_test_contract();
    let mut account = Account::random();
    let tx = account.get_deploy_tx(&contract_code, None, TxType::L1 { serial_id: 0 });
    vm.vm.push_transaction(tx.tx.clone());
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed());

    assert!(vm
        .vm
        .get_used_contracts()
        .contains(&h256_to_u256(tx.bytecode_hash)));

    // Note: `Default_AA` will be in the list of used contracts if L2 tx is used
    assert_eq!(
        vm.vm
            .get_used_contracts()
            .into_iter()
            .collect::<HashSet<U256>>(),
        known_bytecodes_without_aa_code(&vm.vm)
            .keys()
            .cloned()
            .collect::<HashSet<U256>>()
    );

    // create push and execute some non-empty factory deps transaction that fails
    // (`known_bytecodes` will be updated but we expect `get_used_contracts()` to not be updated)

    let calldata = [1, 2, 3];
    let big_calldata: Vec<u8> = calldata
        .iter()
        .cycle()
        .take(calldata.len() * 1024)
        .cloned()
        .collect();
    let account2 = Account::random();
    let tx2 = account2.get_l1_tx(
        Execute {
            contract_address: Some(CONTRACT_DEPLOYER_ADDRESS),
            calldata: big_calldata,
            value: Default::default(),
            factory_deps: vec![vec![1; 32]],
        },
        1,
    );

    vm.vm.push_transaction(tx2.clone());

    let res2 = vm.vm.execute(VmExecutionMode::OneTx);

    assert!(res2.result.is_failed());

    for factory_dep in tx2.execute.factory_deps {
        let hash = hash_bytecode(&factory_dep);
        let hash_to_u256 = h256_to_u256(hash);
        assert!(known_bytecodes_without_aa_code(&vm.vm)
            .keys()
            .contains(&hash_to_u256));
        assert!(!vm.vm.get_used_contracts().contains(&hash_to_u256));
    }
}

#[test]
fn test_contract_is_used_right_after_prepare_to_decommit() {
    let mut vm = VmTesterBuilder::new(HistoryDisabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    assert!(vm.vm.get_used_contracts().is_empty());

    let bytecode_hash =
        U256::from_str("0x100067ff3124f394104ab03481f7923f0bc4029a2aa9d41cc1d848c81257185")
            .unwrap();
    vm.vm
        .state
        .decommittment_processor
        .populate(vec![(bytecode_hash, vec![])], Timestamp(0));

    let header = hex::decode("0100067f").unwrap();
    let normalized_preimage =
        hex::decode("f3124f394104ab03481f7923f0bc4029a2aa9d41cc1d848c81257185").unwrap();
    vm.vm
        .state
        .decommittment_processor
        .prepare_to_decommit(
            0,
            DecommittmentQuery {
                header: VersionedHashHeader(header.try_into().unwrap()),
                normalized_preimage: VersionedHashNormalizedPreimage(
                    normalized_preimage.try_into().unwrap(),
                ),
                timestamp: Timestamp(0),
                memory_page: MemoryPage(0),
                decommitted_length: 0,
                is_fresh: false,
            },
        )
        .unwrap();

    assert_eq!(vm.vm.get_used_contracts(), vec![bytecode_hash]);
}

fn known_bytecodes_without_aa_code<S: WriteStorage, H: HistoryMode>(
    vm: &Vm<S, H>,
) -> HashMap<U256, Vec<U256>> {
    let mut known_bytecodes_without_aa_code = vm
        .state
        .decommittment_processor
        .known_bytecodes
        .inner()
        .clone();

    known_bytecodes_without_aa_code
        .remove(&h256_to_u256(BASE_SYSTEM_CONTRACTS.default_aa.hash))
        .unwrap();

    known_bytecodes_without_aa_code
}
