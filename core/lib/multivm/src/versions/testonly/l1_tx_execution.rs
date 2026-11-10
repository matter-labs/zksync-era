use assert_matches::assert_matches;
use ethabi::Token;
use zksync_contracts::l1_messenger_contract;
use zksync_system_constants::{
    BOOTLOADER_ADDRESS, L1_MESSENGER_ADDRESS, SYSTEM_CONTEXT_ADDRESS,
    SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
};
use zksync_test_contracts::{TestContract, TxType};
use zksync_types::{
    bytecode::BytecodeHash,
    get_code_key, get_known_code_key, h256_to_u256,
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    u256_to_h256,
    utils::storage_key_for_eth_balance,
    web3::keccak256,
    AccountTreeId, Address, Execute, ExecuteTransactionCommon, StorageKey, H256, U256,
};

use super::{
    default_pubdata_builder, tester::VmTesterBuilder, ContractToDeploy, TestedVm,
    BASE_SYSTEM_CONTRACTS,
};
use crate::{
    interface::{
        ExecutionResult, InspectExecutionMode, TxExecutionMode, VmInterfaceExt, VmRevertReason,
    },
    utils::StorageWritesDeduplicator,
};

pub(crate) fn test_l1_tx_execution<VM: TestedVm>() {
    // In this test, we try to execute a contract deployment from L1
    // Here instead of marking code hash via the bootloader means, we will be
    // using L1->L2 communication, the same it would likely be done during the priority mode.

    // There are always at least 9 initial writes here, because we pay fees from l1:
    // - `totalSupply` of ETH token
    // - balance of the refund recipient
    // - balance of the bootloader
    // - `tx_rolling` hash
    // - `gasPerPubdataByte`
    // - `basePubdataSpent`
    // - rolling hash of L2->L1 logs
    // - transaction number in block counter
    // - L2->L1 log counter in `L1Messenger`

    // TODO(PLA-537): right now we are using 5 slots instead of 9 due to 0 fee for transaction.
    let basic_initial_writes = 5;

    let base_token_asset_id = H256::repeat_byte(0x11);

    let mut vm = VmTesterBuilder::new()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_l1_base_token_minting(base_token_asset_id)
        .with_rich_accounts(1)
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];
    let deploy_tx = account.get_deploy_tx(
        TestContract::counter().bytecode,
        None,
        TxType::L1 { serial_id: 1 },
    );
    let tx_hash = deploy_tx.tx.hash();

    let required_l2_to_l1_logs: Vec<_> = vec![L2ToL1Log {
        shard_id: 0,
        is_service: true,
        tx_number_in_block: 0,
        sender: BOOTLOADER_ADDRESS,
        key: tx_hash,
        value: u256_to_h256(U256::from(1u32)),
    }]
    .into_iter()
    .map(UserL2ToL1Log)
    .collect();

    vm.vm.push_transaction(deploy_tx.tx.clone());

    let res = vm.vm.execute(InspectExecutionMode::OneTx);

    // The code hash of the deployed contract should be marked as republished.
    let known_codes_key = get_known_code_key(&deploy_tx.bytecode_hash);

    // The contract should be deployed successfully.
    let account_code_key = get_code_key(&deploy_tx.address);

    assert!(!res.result.is_failed());

    vm.vm.verify_required_storage(&[
        (known_codes_key, U256::from(1)),
        (account_code_key, h256_to_u256(deploy_tx.bytecode_hash)),
    ]);
    assert_eq!(res.logs.user_l2_to_l1_logs, required_l2_to_l1_logs);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        true,
        None,
        false,
        TxType::L1 { serial_id: 0 },
    );
    vm.vm.push_transaction(tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    let storage_logs = res.logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);

    // Tx panicked
    assert_eq!(res.initial_storage_writes, basic_initial_writes);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        false,
        None,
        false,
        TxType::L1 { serial_id: 0 },
    );
    vm.vm.push_transaction(tx.clone());
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    let storage_logs = res.logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);
    // We changed one slot inside contract.
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 1);

    // No repeated writes
    let repeated_writes = res.repeated_storage_writes;
    assert_eq!(res.repeated_storage_writes, 0);

    vm.vm.push_transaction(tx);
    let storage_logs = vm.vm.execute(InspectExecutionMode::OneTx).logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);
    // We do the same storage write, it will be deduplicated, so still 4 initial write and 0 repeated.
    // But now the base pubdata spent has changed too.
    assert_eq!(res.initial_storage_writes, basic_initial_writes + 1);
    assert_eq!(res.repeated_storage_writes, repeated_writes);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        false,
        Some(10.into()),
        false,
        TxType::L1 { serial_id: 1 },
    );
    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    // Method is not payable tx should fail
    assert!(result.result.is_failed(), "The transaction should fail");

    let res = StorageWritesDeduplicator::apply_on_empty_state(&result.logs.storage_logs);
    assert_eq!(res.initial_storage_writes, basic_initial_writes);
    assert_eq!(res.repeated_storage_writes, 2);
}

pub(crate) fn test_l1_tx_execution_high_gas_limit<VM: TestedVm>() {
    // In this test, we try to execute an L1->L2 transaction with a high gas limit.
    // Usually priority transactions with dangerously gas limit should even pass the checks on the L1,
    // however, they might pass during the transition period to the new fee model, so we check that we can safely process those.

    let mut vm = VmTesterBuilder::new()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];

    let l1_messenger = l1_messenger_contract();

    let contract_function = l1_messenger.function("sendToL1").unwrap();
    let params = [
        // Even a message of size 100k should not be able to be sent by a priority transaction
        Token::Bytes(vec![0u8; 100_000]),
    ];
    let calldata = contract_function.encode_input(&params).unwrap();

    let mut tx = account.get_l1_tx(
        Execute {
            contract_address: Some(L1_MESSENGER_ADDRESS),
            value: 0.into(),
            factory_deps: vec![],
            calldata,
        },
        0,
    );

    if let ExecuteTransactionCommon::L1(data) = &mut tx.common_data {
        // Using some large gas limit
        data.gas_limit = 300_000_000.into();
    } else {
        unreachable!()
    };

    vm.vm.push_transaction(tx);

    let res = vm.vm.execute(InspectExecutionMode::OneTx);

    assert!(res.result.is_failed(), "The transaction should've failed");
}

pub(crate) fn test_l1_tx_execution_gas_estimation_with_low_gas<VM: TestedVm>() {
    let counter_contract = TestContract::counter().bytecode.to_vec();
    let counter_address = Address::repeat_byte(0x11);
    let mut vm = VmTesterBuilder::new()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::EstimateFee)
        .with_custom_contracts(vec![ContractToDeploy::new(
            counter_contract,
            counter_address,
        )])
        .with_rich_accounts(1)
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];
    let mut tx = account.get_test_contract_transaction(
        counter_address,
        false,
        None,
        false,
        TxType::L1 { serial_id: 0 },
    );
    let ExecuteTransactionCommon::L1(data) = &mut tx.common_data else {
        unreachable!();
    };
    // This gas limit is chosen so that transaction starts getting executed by the bootloader, but then runs out of gas
    // before its execution result is posted.
    data.gas_limit = 15_000.into();

    vm.vm.push_transaction(tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(
        &res.result,
        ExecutionResult::Revert { output: VmRevertReason::General { msg, .. } }
            if msg.contains("reverted with empty reason")
    );
}

/// Bootloader system log keys, see `chainedPriorityTxnHashLogKey()` / `numberOfLayer1TxsLogKey()`.
const CHAINED_PRIORITY_TXN_HASH_KEY: u32 = 2;
const NUMBER_OF_LAYER_1_TXS_KEY: u32 = 3;

fn chain_priority_hashes(hashes: &[H256]) -> H256 {
    hashes.iter().fold(H256::from(keccak256(&[])), |acc, hash| {
        H256::from(keccak256(&[acc.as_bytes(), hash.as_bytes()].concat()))
    })
}

pub(crate) fn test_l1_tx_force_fail<VM: TestedVm>() {
    let base_token_asset_id = H256::repeat_byte(0x11);
    let mut vm = VmTesterBuilder::new()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_l1_base_token_minting(base_token_asset_id)
        .with_rich_accounts(1)
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];

    // Deploy the counter normally so the force-failed tx has a real target.
    let deploy_tx = account.get_deploy_tx(
        TestContract::counter().bytecode,
        None,
        TxType::L1 { serial_id: 0 },
    );
    let deploy_hash = deploy_tx.tx.hash();
    vm.vm.push_transaction(deploy_tx.tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!res.result.is_failed(), "{:?}", res.result);

    // An increment with deposited value and an unknown factory dep. It would succeed if executed.
    let value = U256::from(10);
    let factory_dep = TestContract::load_test().bytecode.to_vec();
    let factory_dep_hash = BytecodeHash::for_bytecode(&factory_dep).value();
    let calldata = TestContract::counter()
        .abi
        .function("incrementWithRevertPayable")
        .unwrap()
        .encode_input(&[Token::Uint(U256::from(1u8)), Token::Bool(false)])
        .unwrap();
    let mut tx = account.get_l1_tx(
        Execute {
            contract_address: Some(deploy_tx.address),
            calldata,
            value,
            factory_deps: vec![factory_dep],
        },
        1,
    );
    let ExecuteTransactionCommon::L1(data) = &mut tx.common_data else {
        unreachable!();
    };
    data.force_fail = true;
    let refund_recipient = data.refund_recipient;
    let tx_hash = tx.hash();

    let counter_slot = StorageKey::new(AccountTreeId::new(deploy_tx.address), H256::zero());
    let rolling_hash_slot = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
    );
    let balance_slot = storage_key_for_eth_balance(&refund_recipient);
    let counter_before = vm.vm.read_storage(counter_slot);
    let rolling_hash_before = u256_to_h256(vm.vm.read_storage(rolling_hash_slot));
    let balance_before = vm.vm.read_storage(balance_slot);

    vm.vm.push_transaction(tx);
    let res = vm.vm.execute(InspectExecutionMode::OneTx);

    // Fails like an out-of-gas L1 tx: empty revert data.
    assert_matches!(
        &res.result,
        ExecutionResult::Revert { output: VmRevertReason::Unknown { function_selector, data } }
            if function_selector.is_empty() && data.is_empty()
    );
    // The status log, then the force-fail marker keyed by keccak256("zksync.bootloader.forceFailedL1TxLog").
    let force_failed_log_key: H256 =
        "0xc347406c856ed927ab49919146282a0c9feb316c83a07792fce67353bcf0e474"
            .parse()
            .unwrap();
    assert_eq!(
        res.logs.user_l2_to_l1_logs,
        vec![
            UserL2ToL1Log(L2ToL1Log {
                shard_id: 0,
                is_service: true,
                tx_number_in_block: 1,
                sender: BOOTLOADER_ADDRESS,
                key: tx_hash,
                value: H256::zero(),
            }),
            UserL2ToL1Log(L2ToL1Log {
                shard_id: 0,
                is_service: true,
                tx_number_in_block: 1,
                sender: BOOTLOADER_ADDRESS,
                key: force_failed_log_key,
                value: tx_hash,
            }),
        ]
    );

    // No user code ran: nothing changed under the target contract.
    assert!(res
        .logs
        .storage_logs
        .iter()
        .all(|log| log.log.key.address() != &deploy_tx.address));
    assert_eq!(vm.vm.read_storage(counter_slot), counter_before);

    // Preparation still happened: factory dep marked known, hash appended to the L2 block.
    assert_eq!(
        vm.vm.read_storage(get_known_code_key(&factory_dep_hash)),
        U256::one()
    );
    let expected_rolling_hash =
        keccak256(&[rolling_hash_before.as_bytes(), tx_hash.as_bytes()].concat());
    assert_eq!(
        vm.vm.read_storage(rolling_hash_slot),
        h256_to_u256(H256::from(expected_rolling_hash))
    );

    // `get_l1_tx` uses a zero gas price, so the whole deposit is refunded.
    assert_eq!(vm.vm.read_storage(balance_slot), balance_before + value);

    // The priority queue still advanced past both txs.
    let batch_result = vm.vm.finish_batch(default_pubdata_builder());
    assert!(!batch_result.block_tip_execution_result.result.is_failed());
    let system_log = |key: u32| {
        batch_result
            .block_tip_execution_result
            .logs
            .system_l2_to_l1_logs
            .iter()
            .find(|log| log.0.key == u256_to_h256(key.into()))
            .unwrap_or_else(|| panic!("no system log with key {key}"))
            .0
            .value
    };
    assert_eq!(
        system_log(CHAINED_PRIORITY_TXN_HASH_KEY),
        chain_priority_hashes(&[deploy_hash, tx_hash])
    );
    assert_eq!(
        system_log(NUMBER_OF_LAYER_1_TXS_KEY),
        H256::from_low_u64_be(2)
    );
}
