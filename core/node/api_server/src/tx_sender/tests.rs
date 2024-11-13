//! Tests for the transaction sender.

use std::{collections::HashMap, time::Duration};

use assert_matches::assert_matches;
use test_casing::{test_casing, Product, TestCases};
use zksync_contracts::{get_loadnext_contract, test_contracts::LoadnextContractExecutionParams};
use zksync_multivm::interface::ExecutionResult;
use zksync_node_fee_model::MockBatchFeeParamsProvider;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{create_l2_block, create_l2_transaction, prepare_recovery_snapshot};
use zksync_system_constants::CODE_ORACLE_ADDRESS;
use zksync_types::{
    api,
    api::state_override::{Bytecode, OverrideAccount, OverrideState},
    get_nonce_key,
    web3::keccak256,
    K256PrivateKey, L1BatchNumber, L2BlockNumber, StorageLog,
};
use zksync_utils::{bytecode::hash_bytecode, u256_to_h256};
use zksync_vm_executor::oneshot::MockOneshotExecutor;

use super::{gas_estimation::GasEstimator, *};
use crate::{
    execution_sandbox::BlockStartInfo,
    testonly::{
        inflate_bytecode, read_counter_contract_bytecode, read_expensive_contract_bytecode,
        read_infinite_loop_contract_bytecode, read_precompiles_contract_bytecode, TestAccount,
        COUNTER_CONTRACT_ADDRESS, EXPENSIVE_CONTRACT_ADDRESS, INFINITE_LOOP_CONTRACT_ADDRESS,
        LOAD_TEST_ADDRESS, PRECOMPILES_CONTRACT_ADDRESS,
    },
    web3::testonly::create_test_tx_sender,
};

/// Initial pivot multiplier empirically sufficient for most tx types.
const DEFAULT_MULTIPLIER: f64 = 64.0 / 63.0;

#[tokio::test]
async fn getting_nonce_for_account() {
    let l2_chain_id = L2ChainId::default();
    let test_address = Address::repeat_byte(1);
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    // Manually insert a nonce for the address.
    let nonce_key = get_nonce_key(&test_address);
    let nonce_log = StorageLog::new_write_log(nonce_key, H256::from_low_u64_be(123));
    storage
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &[nonce_log])
        .await
        .unwrap();

    let tx_executor = MockOneshotExecutor::default();
    let tx_executor = SandboxExecutor::mock(tx_executor).await;
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(123));

    // Insert another L2 block with a new nonce log.
    storage
        .blocks_dal()
        .insert_l2_block(&create_l2_block(1))
        .await
        .unwrap();
    let nonce_log = StorageLog {
        value: H256::from_low_u64_be(321),
        ..nonce_log
    };
    storage
        .storage_logs_dal()
        .insert_storage_logs(L2BlockNumber(1), &[nonce_log])
        .await
        .unwrap();

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(321));
    let missing_address = Address::repeat_byte(0xff);
    let nonce = tx_sender.get_expected_nonce(missing_address).await.unwrap();
    assert_eq!(nonce, Nonce(0));
}

#[tokio::test]
async fn getting_nonce_for_account_after_snapshot_recovery() {
    const SNAPSHOT_L2_BLOCK_NUMBER: L2BlockNumber = L2BlockNumber(42);

    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let test_address = Address::repeat_byte(1);
    let other_address = Address::repeat_byte(2);
    let nonce_logs = [
        StorageLog::new_write_log(get_nonce_key(&test_address), H256::from_low_u64_be(123)),
        StorageLog::new_write_log(get_nonce_key(&other_address), H256::from_low_u64_be(25)),
    ];
    prepare_recovery_snapshot(
        &mut storage,
        L1BatchNumber(23),
        SNAPSHOT_L2_BLOCK_NUMBER,
        &nonce_logs,
    )
    .await;

    let l2_chain_id = L2ChainId::default();
    let tx_executor = MockOneshotExecutor::default();
    let tx_executor = SandboxExecutor::mock(tx_executor).await;
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;

    storage
        .blocks_dal()
        .insert_l2_block(&create_l2_block(SNAPSHOT_L2_BLOCK_NUMBER.0 + 1))
        .await
        .unwrap();
    let new_nonce_logs = vec![StorageLog::new_write_log(
        get_nonce_key(&test_address),
        H256::from_low_u64_be(321),
    )];
    storage
        .storage_logs_dal()
        .insert_storage_logs(SNAPSHOT_L2_BLOCK_NUMBER + 1, &new_nonce_logs)
        .await
        .unwrap();

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(321));
    let nonce = tx_sender.get_expected_nonce(other_address).await.unwrap();
    assert_eq!(nonce, Nonce(25));
    let missing_address = Address::repeat_byte(0xff);
    let nonce = tx_sender.get_expected_nonce(missing_address).await.unwrap();
    assert_eq!(nonce, Nonce(0));
}

#[tokio::test]
async fn submitting_tx_requires_one_connection() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let l2_chain_id = L2ChainId::default();
    let fee_input = MockBatchFeeParamsProvider::default()
        .get_batch_fee_input_scaled(1.0, 1.0)
        .await
        .unwrap();
    let (base_fee, gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());
    let tx = create_l2_transaction(base_fee, gas_per_pubdata);
    let tx_hash = tx.hash();

    // Manually set sufficient balance for the tx initiator.
    let balance_key = storage_key_for_eth_balance(&tx.initiator_account());
    let storage_log = StorageLog::new_write_log(balance_key, u256_to_h256(U256::one() << 64));
    storage
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &[storage_log])
        .await
        .unwrap();
    drop(storage);

    let mut tx_executor = MockOneshotExecutor::default();
    tx_executor.set_tx_responses(move |received_tx, _| {
        assert_eq!(received_tx.hash(), tx_hash);
        ExecutionResult::Success { output: vec![] }
    });
    let tx_executor = SandboxExecutor::mock(tx_executor).await;
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;

    let submission_result = tx_sender.submit_tx(tx).await.unwrap();
    assert_matches!(submission_result.0, L2TxSubmissionResult::Added);

    let mut storage = pool.connection().await.unwrap();
    storage
        .transactions_web3_dal()
        .get_transaction_by_hash(tx_hash, l2_chain_id)
        .await
        .unwrap()
        .expect("transaction is not persisted");
}

#[tokio::test]
async fn eth_call_requires_single_connection() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut storage, &genesis_params)
        .await
        .unwrap();
    let start_info = BlockStartInfo::new(&mut storage, Duration::MAX)
        .await
        .unwrap();
    let block_id = api::BlockId::Number(api::BlockNumber::Latest);
    let block_args = BlockArgs::new(&mut storage, block_id, &start_info)
        .await
        .unwrap();
    drop(storage);

    let tx = create_l2_transaction(10, 100);
    let tx_hash = tx.hash();

    let mut tx_executor = MockOneshotExecutor::default();
    tx_executor.set_call_responses(move |received_tx, _| {
        assert_eq!(received_tx.hash(), tx_hash);
        ExecutionResult::Success {
            output: b"success!".to_vec(),
        }
    });
    let tx_executor = SandboxExecutor::mock(tx_executor).await;
    let (tx_sender, _) = create_test_tx_sender(
        pool.clone(),
        genesis_params.config().l2_chain_id,
        tx_executor,
    )
    .await;
    let call_overrides = CallOverrides {
        enforced_base_fee: None,
    };
    let output = tx_sender
        .eth_call(block_args, call_overrides, tx, None)
        .await
        .unwrap();
    assert_eq!(output, b"success!");
}

async fn create_real_tx_sender() -> TxSender {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut storage, &genesis_params)
        .await
        .unwrap();
    drop(storage);

    let genesis_config = genesis_params.config();
    let executor_options = SandboxExecutorOptions::new(
        genesis_config.l2_chain_id,
        AccountTreeId::new(genesis_config.fee_account),
        u32::MAX,
    )
    .await
    .unwrap();

    let pg_caches = PostgresStorageCaches::new(1, 1);
    let tx_executor = SandboxExecutor::real(executor_options, pg_caches, usize::MAX);
    create_test_tx_sender(pool, genesis_params.config().l2_chain_id, tx_executor)
        .await
        .0
}

#[tokio::test]
async fn initial_gas_estimation_is_somewhat_accurate() {
    let tx_sender = create_real_tx_sender().await;

    let alice = K256PrivateKey::random();
    let transfer_value = U256::from(1_000_000_000);
    let account_overrides = OverrideAccount {
        balance: Some(transfer_value * 2),
        ..OverrideAccount::default()
    };
    let state_override = StateOverride::new(HashMap::from([(alice.address(), account_overrides)]));
    // fee params don't matter; we adjust via `adjust_transaction_fee()`
    let tx = alice.create_transfer(transfer_value, 55, 555);

    let mut estimator = GasEstimator::new(&tx_sender, tx.into(), Some(state_override))
        .await
        .unwrap();
    estimator.adjust_transaction_fee();
    let initial_estimate = estimator.initialize().await.unwrap();
    assert!(initial_estimate.gas_charged_for_pubdata > 0);
    assert!(initial_estimate.operator_overhead > 0);
    let total_gas_charged = initial_estimate.total_gas_charged.unwrap();
    assert!(
        total_gas_charged
            > initial_estimate.gas_charged_for_pubdata + initial_estimate.operator_overhead,
        "{initial_estimate:?}"
    );

    // Check that a transaction fails if supplied with the lower bound.
    let lower_bound = initial_estimate.lower_gas_bound_without_overhead().unwrap()
        + initial_estimate.operator_overhead;
    assert!(lower_bound < total_gas_charged, "{initial_estimate:?}");
    let (vm_result, _) = estimator.unadjusted_step(lower_bound).await.unwrap();
    assert!(vm_result.result.is_failed(), "{:?}", vm_result.result);

    // A slightly larger limit should work.
    let initial_pivot = total_gas_charged * 64 / 63;
    let (vm_result, _) = estimator.unadjusted_step(initial_pivot).await.unwrap();
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

const LOAD_TEST_CASES: TestCases<LoadnextContractExecutionParams> = test_casing::cases! {[
    LoadnextContractExecutionParams::default(),
    // No storage modification
    LoadnextContractExecutionParams {
        writes: 0,
        events: 0,
        ..LoadnextContractExecutionParams::default()
    },
    // Moderately deep recursion (very deep recursion is tested separately)
    LoadnextContractExecutionParams {
        recursive_calls: 10,
        ..LoadnextContractExecutionParams::default()
    },
    // No deploys
    LoadnextContractExecutionParams {
        deploys: 0,
        ..LoadnextContractExecutionParams::default()
    },
    // Lots of deploys
    LoadnextContractExecutionParams {
        deploys: 10,
        ..LoadnextContractExecutionParams::default()
    },
]};

#[test_casing(5, LOAD_TEST_CASES)]
#[tokio::test]
async fn initial_estimate_for_load_test_transaction(tx_params: LoadnextContractExecutionParams) {
    let alice = K256PrivateKey::random();
    // Set the array length in the load test contract to 100, so that reads don't fail.
    let load_test_state = HashMap::from([(H256::zero(), H256::from_low_u64_be(100))]);
    let load_test_overrides = OverrideAccount {
        code: Some(Bytecode::new(get_loadnext_contract().bytecode).unwrap()),
        state: Some(OverrideState::State(load_test_state)),
        ..OverrideAccount::default()
    };
    let state_override =
        StateOverride::new(HashMap::from([(LOAD_TEST_ADDRESS, load_test_overrides)]));
    let tx = alice.create_load_test_tx(tx_params);

    test_initial_estimate(state_override, tx, DEFAULT_MULTIPLIER).await;
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn initial_estimate_for_deep_recursion(with_reads: bool) {
    let alice = K256PrivateKey::random();
    let load_test_state = HashMap::from([(H256::zero(), H256::from_low_u64_be(100))]);
    let load_test_overrides = OverrideAccount {
        code: Some(Bytecode::new(get_loadnext_contract().bytecode).unwrap()),
        state: Some(OverrideState::State(load_test_state)),
        ..OverrideAccount::default()
    };
    let state_override =
        StateOverride::new(HashMap::from([(LOAD_TEST_ADDRESS, load_test_overrides)]));

    // Reads are chosen because they represent the worst case. Reads don't influence the amount of pubdata;
    // i.e., they don't make it easier to execute a transaction because of additional gas reserved for pubdata.
    // OTOH, reads still increase the amount of computational gas used on each nested call.
    //
    // Initial pivot multipliers below are the smallest ones with 0.1 precision. `DEFAULT_MULTIPLIER` works for smaller
    // recursion depths because the transaction emits enough pubdata to cover gas deductions due to the 63/64 rule.
    let depths_and_multipliers: &[_] = if with_reads {
        &[(25, DEFAULT_MULTIPLIER), (50, 1.2), (75, 1.4), (100, 1.7)]
    } else {
        &[
            (50, DEFAULT_MULTIPLIER),
            (75, 1.2),
            (100, 1.4),
            (125, 1.7),
            (150, 2.1),
        ]
    };
    for &(recursion_depth, multiplier) in depths_and_multipliers {
        println!("Testing recursion depth {recursion_depth}");
        let tx = alice.create_load_test_tx(LoadnextContractExecutionParams {
            recursive_calls: recursion_depth,
            reads: if with_reads { 10 } else { 0 },
            ..LoadnextContractExecutionParams::empty()
        });
        test_initial_estimate(state_override.clone(), tx, multiplier).await;
    }
}

#[tokio::test]
async fn initial_estimate_for_deep_recursion_with_large_bytecode() {
    let alice = K256PrivateKey::random();
    let mut contract_bytecode = get_loadnext_contract().bytecode;
    inflate_bytecode(&mut contract_bytecode, 50_000);
    let load_test_overrides = OverrideAccount {
        code: Some(Bytecode::new(contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };
    let state_override =
        StateOverride::new(HashMap::from([(LOAD_TEST_ADDRESS, load_test_overrides)]));
    let tx = alice.create_load_test_tx(LoadnextContractExecutionParams {
        recursive_calls: 100,
        ..LoadnextContractExecutionParams::empty()
    });

    test_initial_estimate(state_override, tx, 1.35).await;
}

/// Tests the lower bound and initial pivot extracted from the initial estimate (one with effectively infinite gas amount).
/// Returns the VM result for a VM run with the initial pivot.
async fn test_initial_estimate(
    state_override: StateOverride,
    tx: L2Tx,
    initial_pivot_multiplier: f64,
) -> VmExecutionResultAndLogs {
    let tx_sender = create_real_tx_sender().await;
    let mut estimator = GasEstimator::new(&tx_sender, tx.into(), Some(state_override))
        .await
        .unwrap();
    estimator.adjust_transaction_fee();
    let initial_estimate = estimator.initialize().await.unwrap();

    let lower_bound = initial_estimate.lower_gas_bound_without_overhead().unwrap()
        + initial_estimate.operator_overhead;
    let (vm_result, _) = estimator.unadjusted_step(lower_bound).await.unwrap();
    assert!(vm_result.result.is_failed(), "{:?}", vm_result.result);

    // A slightly larger limit should work.
    let initial_pivot =
        (initial_estimate.total_gas_charged.unwrap() as f64 * initial_pivot_multiplier) as u64;
    let (vm_result, _) = estimator.unadjusted_step(initial_pivot).await.unwrap();
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
    vm_result
}

async fn test_initial_estimate_error(state_override: StateOverride, tx: L2Tx) -> SubmitTxError {
    let tx_sender = create_real_tx_sender().await;
    let mut estimator = GasEstimator::new(&tx_sender, tx.into(), Some(state_override))
        .await
        .unwrap();
    estimator.adjust_transaction_fee();
    estimator.initialize().await.unwrap_err()
}

/// Estimates both transactions with initial writes and cleanup.
#[test_casing(4, [10, 50, 200, 1_000])]
#[tokio::test]
async fn initial_estimate_for_expensive_contract(write_count: usize) {
    let alice = K256PrivateKey::random();
    let contract_bytecode = read_expensive_contract_bytecode();
    let mut contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };
    let state_override = StateOverride::new(HashMap::from([(
        EXPENSIVE_CONTRACT_ADDRESS,
        contract_overrides.clone(),
    )]));
    let tx = alice.create_expensive_tx(write_count);

    let vm_result = test_initial_estimate(state_override, tx, DEFAULT_MULTIPLIER).await;

    let contract_logs = vm_result.logs.storage_logs.into_iter().filter_map(|log| {
        (*log.log.key.address() == EXPENSIVE_CONTRACT_ADDRESS)
            .then_some((*log.log.key.key(), log.log.value))
    });
    let contract_logs: HashMap<_, _> = contract_logs.collect();
    assert!(contract_logs.len() >= write_count, "{contract_logs:?}");
    contract_overrides.state = Some(OverrideState::StateDiff(contract_logs));

    let state_override = StateOverride::new(HashMap::from([(
        EXPENSIVE_CONTRACT_ADDRESS,
        contract_overrides,
    )]));
    let tx = alice.create_expensive_cleanup_tx();

    test_initial_estimate(state_override, tx, DEFAULT_MULTIPLIER).await;
}

#[tokio::test]
async fn initial_estimate_for_code_oracle_tx() {
    let alice = K256PrivateKey::random();
    let contract_bytecode = read_precompiles_contract_bytecode();
    let contract_bytecode_hash = hash_bytecode(&contract_bytecode);
    let contract_keccak_hash = H256(keccak256(&contract_bytecode));
    let contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };

    // Add another contract that is never executed, but has a large bytecode.
    let huge_contact_address = Address::repeat_byte(23);
    let huge_contract_bytecode = vec![0_u8; 10_001 * 32];
    let huge_contract_bytecode_hash = hash_bytecode(&huge_contract_bytecode);
    let huge_contract_keccak_hash = H256(keccak256(&huge_contract_bytecode));
    let huge_contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(huge_contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };

    let state_override = StateOverride::new(HashMap::from([
        (PRECOMPILES_CONTRACT_ADDRESS, contract_overrides),
        (huge_contact_address, huge_contract_overrides),
    ]));

    // Test contracts that are already decommitted when requested from the precompiles test contract.
    let genesis_params = GenesisParams::mock();
    let code_oracle_bytecode = genesis_params
        .system_contracts()
        .iter()
        .find_map(|contract| {
            (*contract.account_id.address() == CODE_ORACLE_ADDRESS).then_some(&contract.bytecode)
        })
        .expect("no code oracle");
    let code_oracle_bytecode_hash = hash_bytecode(code_oracle_bytecode);
    let code_oracle_keccak_hash = H256(keccak256(code_oracle_bytecode));

    let warm_bytecode_hashes = [
        (code_oracle_bytecode_hash, code_oracle_keccak_hash),
        (contract_bytecode_hash, contract_keccak_hash),
    ];
    let mut decomitter_stats = 0.0;
    for (hash, keccak_hash) in warm_bytecode_hashes {
        println!("Testing bytecode: {hash:?}");
        let tx = alice.create_code_oracle_tx(hash, keccak_hash);
        let vm_result = test_initial_estimate(state_override.clone(), tx, DEFAULT_MULTIPLIER).await;
        let stats = &vm_result.statistics.circuit_statistic;
        decomitter_stats = stats.code_decommitter.max(decomitter_stats);
    }
    assert!(decomitter_stats > 0.0);

    println!("Testing large bytecode");
    let tx = alice.create_code_oracle_tx(huge_contract_bytecode_hash, huge_contract_keccak_hash);
    let vm_result = test_initial_estimate(state_override, tx, 1.05).await;
    // Sanity check: the transaction should spend significantly more on decommitment compared to previous ones
    let new_decomitter_stats = vm_result.statistics.circuit_statistic.code_decommitter;
    assert!(
        new_decomitter_stats > decomitter_stats * 1.5,
        "old={decomitter_stats}, new={new_decomitter_stats}"
    );
}

#[tokio::test]
async fn initial_estimate_with_large_free_bytecode() {
    let alice = K256PrivateKey::random();
    let mut contract_bytecode = read_precompiles_contract_bytecode();
    inflate_bytecode(&mut contract_bytecode, 50_000);
    let contract_bytecode_hash = hash_bytecode(&contract_bytecode);
    let contract_keccak_hash = H256(keccak256(&contract_bytecode));
    let contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };

    let state_override = StateOverride::new(HashMap::from([(
        PRECOMPILES_CONTRACT_ADDRESS,
        contract_overrides,
    )]));
    // Ask the test contract to decommit itself. This should refund the decommit costs, but it will be charged at first.
    let tx = alice.create_code_oracle_tx(contract_bytecode_hash, contract_keccak_hash);
    test_initial_estimate(state_override, tx, 1.05).await;
}

#[tokio::test]
async fn revert_during_initial_estimate() {
    let alice = K256PrivateKey::random();
    let contract_bytecode = read_counter_contract_bytecode();
    let contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };
    let state_override = StateOverride::new(HashMap::from([(
        COUNTER_CONTRACT_ADDRESS,
        contract_overrides,
    )]));

    let tx = alice.create_reverting_counter_tx();
    let err = test_initial_estimate_error(state_override, tx).await;
    let SubmitTxError::ExecutionReverted(err, _) = err else {
        panic!("Unexpected error: {err:?}");
    };
    assert_eq!(err, "This method always reverts");
}

#[tokio::test]
async fn out_of_gas_during_initial_estimate() {
    let alice = K256PrivateKey::random();
    let contract_bytecode = read_infinite_loop_contract_bytecode();
    let contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };
    let state_override = StateOverride::new(HashMap::from([(
        INFINITE_LOOP_CONTRACT_ADDRESS,
        contract_overrides,
    )]));

    let tx = alice.create_infinite_loop_tx();
    let err = test_initial_estimate_error(state_override, tx).await;
    // Unfortunately, we don't provide human-readable out-of-gas errors at the time
    assert_matches!(err, SubmitTxError::ExecutionReverted(msg, _) if msg.is_empty());
}

#[tokio::test]
async fn insufficient_funds_error_for_transfer() {
    let tx_sender = create_real_tx_sender().await;

    let alice = K256PrivateKey::random();
    let transfer_value = 1_000_000_000.into();
    // fee params don't matter; they should be overwritten by the estimation logic
    let tx = alice.create_transfer(transfer_value, 55, 555);
    let fee_scale_factor = 1.0;
    // Without overrides, the transaction should fail because of insufficient balance.
    let err = tx_sender
        .get_txs_fee_in_wei(
            tx.clone().into(),
            fee_scale_factor,
            1_000,
            None,
            BinarySearchKind::Full,
        )
        .await
        .unwrap_err();
    assert_matches!(err, SubmitTxError::InsufficientFundsForTransfer);
}

async fn test_estimating_gas(
    state_override: StateOverride,
    tx: L2Tx,
    acceptable_overestimation: u64,
) {
    let tx_sender = create_real_tx_sender().await;

    let fee_scale_factor = 1.0;
    let fee = tx_sender
        .get_txs_fee_in_wei(
            tx.clone().into(),
            fee_scale_factor,
            acceptable_overestimation,
            Some(state_override.clone()),
            BinarySearchKind::Full,
        )
        .await
        .unwrap();
    // Sanity-check gas limit
    let gas_limit_after_full_search = u64::try_from(fee.gas_limit).unwrap();
    assert!(
        (10_000..10_000_000).contains(&gas_limit_after_full_search),
        "{fee:?}"
    );

    let fee = tx_sender
        .get_txs_fee_in_wei(
            tx.into(),
            fee_scale_factor,
            acceptable_overestimation,
            Some(state_override.clone()),
            BinarySearchKind::Optimized,
        )
        .await
        .unwrap();
    let gas_limit_after_optimized_search = u64::try_from(fee.gas_limit).unwrap();

    let diff = gas_limit_after_full_search.abs_diff(gas_limit_after_optimized_search);
    assert!(
        diff <= acceptable_overestimation,
        "full={gas_limit_after_full_search}, optimized={gas_limit_after_optimized_search}"
    );
}

#[test_casing(3, [0, 100, 1_000])]
#[tokio::test]
async fn estimating_gas_for_transfer(acceptable_overestimation: u64) {
    let alice = K256PrivateKey::random();
    let transfer_value = 1_000_000_000.into();
    let account_overrides = OverrideAccount {
        balance: Some(transfer_value * 2),
        ..OverrideAccount::default()
    };
    let state_override = StateOverride::new(HashMap::from([(alice.address(), account_overrides)]));
    // fee params don't matter; they should be overwritten by the estimation logic
    let tx = alice.create_transfer(transfer_value, 55, 555);

    test_estimating_gas(state_override, tx, acceptable_overestimation).await;
}

#[test_casing(10, Product((LOAD_TEST_CASES, [0, 100])))]
#[tokio::test]
async fn estimating_gas_for_load_test_tx(
    tx_params: LoadnextContractExecutionParams,
    acceptable_overestimation: u64,
) {
    let alice = K256PrivateKey::random();
    // Set the array length in the load test contract to 100, so that reads don't fail.
    let load_test_state = HashMap::from([(H256::zero(), H256::from_low_u64_be(100))]);
    let load_test_overrides = OverrideAccount {
        code: Some(Bytecode::new(get_loadnext_contract().bytecode).unwrap()),
        state: Some(OverrideState::State(load_test_state)),
        ..OverrideAccount::default()
    };
    let state_override =
        StateOverride::new(HashMap::from([(LOAD_TEST_ADDRESS, load_test_overrides)]));
    let tx = alice.create_load_test_tx(tx_params);

    test_estimating_gas(state_override, tx, acceptable_overestimation).await;
}

#[test_casing(4, [10, 50, 100, 200])]
#[tokio::test]
async fn estimating_gas_for_expensive_txs(write_count: usize) {
    let alice = K256PrivateKey::random();
    let contract_bytecode = read_expensive_contract_bytecode();
    let contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };
    let state_override = StateOverride::new(HashMap::from([(
        EXPENSIVE_CONTRACT_ADDRESS,
        contract_overrides.clone(),
    )]));
    let tx = alice.create_expensive_tx(write_count);

    test_estimating_gas(state_override, tx, 0).await;
}

#[tokio::test]
async fn estimating_gas_for_code_oracle_tx() {
    let alice = K256PrivateKey::random();
    let contract_bytecode = read_precompiles_contract_bytecode();
    let contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };

    // Add another contract that is never executed, but has a large bytecode.
    let huge_contact_address = Address::repeat_byte(23);
    let huge_contract_bytecode = vec![0_u8; 10_001 * 32];
    let huge_contract_bytecode_hash = hash_bytecode(&huge_contract_bytecode);
    let huge_contract_keccak_hash = H256(keccak256(&huge_contract_bytecode));
    let huge_contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(huge_contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };

    let state_override = StateOverride::new(HashMap::from([
        (PRECOMPILES_CONTRACT_ADDRESS, contract_overrides),
        (huge_contact_address, huge_contract_overrides),
    ]));
    let tx = alice.create_code_oracle_tx(huge_contract_bytecode_hash, huge_contract_keccak_hash);

    test_estimating_gas(state_override, tx, 0).await;
}

#[tokio::test]
async fn estimating_gas_for_reverting_tx() {
    let alice = K256PrivateKey::random();
    let contract_bytecode = read_counter_contract_bytecode();
    let contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };
    let state_override = StateOverride::new(HashMap::from([(
        COUNTER_CONTRACT_ADDRESS,
        contract_overrides,
    )]));

    let tx = alice.create_reverting_counter_tx();
    let tx_sender = create_real_tx_sender().await;

    let fee_scale_factor = 1.0;
    let acceptable_overestimation = 0;
    for binary_search_kind in [BinarySearchKind::Full, BinarySearchKind::Optimized] {
        let err = tx_sender
            .get_txs_fee_in_wei(
                tx.clone().into(),
                fee_scale_factor,
                acceptable_overestimation,
                Some(state_override.clone()),
                binary_search_kind,
            )
            .await
            .unwrap_err();
        assert_matches!(err, SubmitTxError::ExecutionReverted(..));
    }
}

#[tokio::test]
async fn estimating_gas_for_infinite_loop_tx() {
    let alice = K256PrivateKey::random();
    let contract_bytecode = read_infinite_loop_contract_bytecode();
    let contract_overrides = OverrideAccount {
        code: Some(Bytecode::new(contract_bytecode).unwrap()),
        ..OverrideAccount::default()
    };
    let state_override = StateOverride::new(HashMap::from([(
        INFINITE_LOOP_CONTRACT_ADDRESS,
        contract_overrides,
    )]));

    let tx = alice.create_infinite_loop_tx();
    let tx_sender = create_real_tx_sender().await;

    let fee_scale_factor = 1.0;
    let acceptable_overestimation = 0;
    for binary_search_kind in [BinarySearchKind::Full, BinarySearchKind::Optimized] {
        let err = tx_sender
            .get_txs_fee_in_wei(
                tx.clone().into(),
                fee_scale_factor,
                acceptable_overestimation,
                Some(state_override.clone()),
                binary_search_kind,
            )
            .await
            .unwrap_err();
        assert_matches!(err, SubmitTxError::ExecutionReverted(msg, _) if msg.is_empty());
    }
}
