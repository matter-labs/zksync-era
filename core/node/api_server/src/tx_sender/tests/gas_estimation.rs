//! Tests for gas estimation (mostly with the real oneshot VM executor).

use std::collections::HashMap;

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use zksync_system_constants::CODE_ORACLE_ADDRESS;
use zksync_types::{
    api::state_override::{OverrideAccount, OverrideState},
    web3::keccak256,
    K256PrivateKey,
};
use zksync_utils::bytecode::hash_bytecode;

use super::*;
use crate::{
    testonly::{StateBuilder, TestAccount},
    tx_sender::gas_estimation::GasEstimator,
};

/// Initial pivot multiplier empirically sufficient for most tx types.
const DEFAULT_MULTIPLIER: f64 = 64.0 / 63.0;

#[tokio::test]
async fn initial_gas_estimation_is_somewhat_accurate() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;

    let alice = K256PrivateKey::random();
    let transfer_value = U256::from(1_000_000_000);
    let account_overrides = OverrideAccount {
        balance: Some(transfer_value * 2),
        ..OverrideAccount::default()
    };
    let state_override = StateOverride::new(HashMap::from([(alice.address(), account_overrides)]));
    let tx = alice.create_transfer(transfer_value);

    let mut estimator = GasEstimator::new(&tx_sender, tx.into(), block_args, Some(state_override))
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

#[test_casing(5, LOAD_TEST_CASES)]
#[tokio::test]
async fn initial_estimate_for_load_test_transaction(tx_params: LoadnextContractExecutionParams) {
    let alice = K256PrivateKey::random();
    // Set the array length in the load test contract to 100, so that reads don't fail.
    let state_override = StateBuilder::default().with_load_test_contract().build();
    let tx = alice.create_load_test_tx(tx_params);

    test_initial_estimate(state_override, tx, DEFAULT_MULTIPLIER).await;
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn initial_estimate_for_deep_recursion(with_reads: bool) {
    let alice = K256PrivateKey::random();
    let state_override = StateBuilder::default().with_load_test_contract().build();

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
    let state_override = StateBuilder::default()
        .with_load_test_contract()
        .inflate_bytecode(StateBuilder::LOAD_TEST_ADDRESS, 50_000)
        .build();
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
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;
    let mut estimator = GasEstimator::new(&tx_sender, tx.into(), block_args, Some(state_override))
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
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;
    let mut estimator = GasEstimator::new(&tx_sender, tx.into(), block_args, Some(state_override))
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
    let mut state_override = StateBuilder::default().with_expensive_contract().build();
    let tx = alice.create_expensive_tx(write_count);

    let vm_result = test_initial_estimate(state_override.clone(), tx, DEFAULT_MULTIPLIER).await;

    let contract_logs = vm_result.logs.storage_logs.into_iter().filter_map(|log| {
        (*log.log.key.address() == StateBuilder::EXPENSIVE_CONTRACT_ADDRESS)
            .then_some((*log.log.key.key(), log.log.value))
    });
    let contract_logs: HashMap<_, _> = contract_logs.collect();
    assert!(contract_logs.len() >= write_count, "{contract_logs:?}");

    state_override
        .get_mut(&StateBuilder::EXPENSIVE_CONTRACT_ADDRESS)
        .unwrap()
        .state = Some(OverrideState::StateDiff(contract_logs));
    let tx = alice.create_expensive_cleanup_tx();
    test_initial_estimate(state_override, tx, DEFAULT_MULTIPLIER).await;
}

#[tokio::test]
async fn initial_estimate_for_code_oracle_tx() {
    let alice = K256PrivateKey::random();
    // Add another contract that is never executed, but has a large bytecode.
    let huge_contact_address = Address::repeat_byte(23);
    let huge_contract_bytecode = vec![0_u8; 10_001 * 32];
    let huge_contract_bytecode_hash = hash_bytecode(&huge_contract_bytecode);
    let huge_contract_keccak_hash = H256(keccak256(&huge_contract_bytecode));

    let state_override = StateBuilder::default()
        .with_precompiles_contract()
        .with_contract(huge_contact_address, huge_contract_bytecode)
        .build();

    let contract_override = state_override
        .get(&StateBuilder::PRECOMPILES_CONTRACT_ADDRESS)
        .unwrap();
    let contract_bytecode = contract_override.code.as_ref().unwrap();
    let contract_bytecode_hash = contract_bytecode.hash();
    let contract_keccak_hash = H256(keccak256(contract_bytecode.as_ref()));

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
    let state_override = StateBuilder::default()
        .with_precompiles_contract()
        .inflate_bytecode(StateBuilder::PRECOMPILES_CONTRACT_ADDRESS, 50_000)
        .build();
    let contract_override = state_override
        .get(&StateBuilder::PRECOMPILES_CONTRACT_ADDRESS)
        .unwrap();
    let contract_bytecode = contract_override.code.as_ref().unwrap();
    let contract_bytecode_hash = contract_bytecode.hash();
    let contract_keccak_hash = H256(keccak256(contract_bytecode.as_ref()));

    // Ask the test contract to decommit itself. This should refund the decommit costs, but it will be charged at first.
    let tx = alice.create_code_oracle_tx(contract_bytecode_hash, contract_keccak_hash);
    test_initial_estimate(state_override, tx, 1.05).await;
}

#[tokio::test]
async fn revert_during_initial_estimate() {
    let alice = K256PrivateKey::random();
    let state_override = StateBuilder::default().with_counter_contract(0).build();

    let tx = alice.create_counter_tx(1.into(), true);
    let err = test_initial_estimate_error(state_override, tx).await;
    let SubmitTxError::ExecutionReverted(err, _) = err else {
        panic!("Unexpected error: {err:?}");
    };
    assert_eq!(err, "This method always reverts");
}

#[tokio::test]
async fn out_of_gas_during_initial_estimate() {
    let alice = K256PrivateKey::random();
    let state_override = StateBuilder::default()
        .with_infinite_loop_contract()
        .build();

    let tx = alice.create_infinite_loop_tx();
    let err = test_initial_estimate_error(state_override, tx).await;
    // Unfortunately, we don't provide human-readable out-of-gas errors at the time
    assert_matches!(err, SubmitTxError::ExecutionReverted(msg, _) if msg.is_empty());
}

#[tokio::test]
async fn insufficient_funds_error_for_transfer() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;

    let alice = K256PrivateKey::random();
    let transferred_value = 1_000_000_000.into();
    let tx = alice.create_transfer(transferred_value);
    let fee_scale_factor = 1.0;
    // Without overrides, the transaction should fail because of insufficient balance.
    let err = tx_sender
        .get_txs_fee_in_wei(
            tx.clone().into(),
            block_args,
            fee_scale_factor,
            1_000,
            None,
            BinarySearchKind::Full,
        )
        .await
        .unwrap_err();
    assert_matches!(
        err,
        SubmitTxError::NotEnoughBalanceForFeeValue(balance, fee, value)
            if balance.is_zero() && fee.is_zero() && value == transferred_value
    );
}

async fn test_estimating_gas(
    state_override: StateOverride,
    tx: L2Tx,
    acceptable_overestimation: u64,
) {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;

    let fee_scale_factor = 1.0;
    let fee = tx_sender
        .get_txs_fee_in_wei(
            tx.clone().into(),
            block_args.clone(),
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
            block_args,
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
    let tx = alice.create_transfer(transfer_value);

    test_estimating_gas(state_override, tx, acceptable_overestimation).await;
}

#[test_casing(10, Product((LOAD_TEST_CASES, [0, 100])))]
#[tokio::test]
async fn estimating_gas_for_load_test_tx(
    tx_params: LoadnextContractExecutionParams,
    acceptable_overestimation: u64,
) {
    let alice = K256PrivateKey::random();
    let state_override = StateBuilder::default().with_load_test_contract().build();
    let tx = alice.create_load_test_tx(tx_params);

    test_estimating_gas(state_override, tx, acceptable_overestimation).await;
}

#[test_casing(4, [10, 50, 100, 200])]
#[tokio::test]
async fn estimating_gas_for_expensive_txs(write_count: usize) {
    let alice = K256PrivateKey::random();
    let state_override = StateBuilder::default().with_expensive_contract().build();
    let tx = alice.create_expensive_tx(write_count);

    test_estimating_gas(state_override, tx, 0).await;
}

#[tokio::test]
async fn estimating_gas_for_code_oracle_tx() {
    let alice = K256PrivateKey::random();
    // Add another contract that is never executed, but has a large bytecode.
    let huge_contact_address = Address::repeat_byte(23);
    let huge_contract_bytecode = vec![0_u8; 10_001 * 32];
    let huge_contract_bytecode_hash = hash_bytecode(&huge_contract_bytecode);
    let huge_contract_keccak_hash = H256(keccak256(&huge_contract_bytecode));

    let state_override = StateBuilder::default()
        .with_precompiles_contract()
        .with_contract(huge_contact_address, huge_contract_bytecode)
        .build();
    let tx = alice.create_code_oracle_tx(huge_contract_bytecode_hash, huge_contract_keccak_hash);

    test_estimating_gas(state_override, tx, 0).await;
}

#[tokio::test]
async fn estimating_gas_for_reverting_tx() {
    let alice = K256PrivateKey::random();
    let state_override = StateBuilder::default().with_counter_contract(0).build();

    let tx = alice.create_counter_tx(1.into(), true);
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;

    let fee_scale_factor = 1.0;
    let acceptable_overestimation = 0;
    for binary_search_kind in [BinarySearchKind::Full, BinarySearchKind::Optimized] {
        let err = tx_sender
            .get_txs_fee_in_wei(
                tx.clone().into(),
                block_args.clone(),
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
    let state_override = StateBuilder::default()
        .with_infinite_loop_contract()
        .build();

    let tx = alice.create_infinite_loop_tx();
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;

    let fee_scale_factor = 1.0;
    let acceptable_overestimation = 0;
    for binary_search_kind in [BinarySearchKind::Full, BinarySearchKind::Optimized] {
        let err = tx_sender
            .get_txs_fee_in_wei(
                tx.clone().into(),
                block_args.clone(),
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
