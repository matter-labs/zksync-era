//! Tests for `eth_call`.

use std::collections::HashMap;

use assert_matches::assert_matches;
use zksync_multivm::interface::ExecutionResult;
use zksync_node_test_utils::create_l2_transaction;
use zksync_types::{
    api::state_override::OverrideAccount, transaction_request::CallRequest, K256PrivateKey,
};

use super::*;
use crate::testonly::{decode_u256_output, Call3Result, Call3Value, StateBuilder, TestAccount};

#[tokio::test]
async fn eth_call_requires_single_connection() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut storage, &genesis_params)
        .await
        .unwrap();
    let block_args = BlockArgs::pending(&mut storage).await.unwrap();
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

async fn test_call(
    tx_sender: &TxSender,
    state_override: StateOverride,
    mut call: CallRequest,
) -> Result<Vec<u8>, SubmitTxError> {
    call.gas = call.gas.max(Some(10_000_000.into()));
    let call = L2Tx::from_request(call.into(), usize::MAX, true).unwrap();

    let mut storage = tx_sender
        .0
        .replica_connection_pool
        .connection()
        .await
        .unwrap();
    let block_args = BlockArgs::pending(&mut storage).await.unwrap();
    drop(storage);
    let call_overrides = CallOverrides {
        enforced_base_fee: None,
    };

    tx_sender
        .eth_call(block_args, call_overrides, call, Some(state_override))
        .await
}

#[tokio::test]
async fn eth_call_with_balance() {
    let alice = K256PrivateKey::random();
    let initial_balance = 123_456_789.into();
    let account_overrides = OverrideAccount {
        balance: Some(initial_balance),
        ..OverrideAccount::default()
    };
    let state_override = StateOverride::new(HashMap::from([(alice.address(), account_overrides)]));

    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let call = alice.query_base_token_balance();
    let output = test_call(&tx_sender, state_override, call).await.unwrap();
    assert_eq!(decode_u256_output(&output), initial_balance);
}

#[tokio::test]
async fn eth_call_with_transfer() {
    let alice = K256PrivateKey::random();
    let transfer_value = 1_000_000_000.into();
    let initial_balance = transfer_value * 5 / 3;
    let state_override = StateBuilder::default()
        .with_multicall3_contract()
        .with_balance(alice.address(), initial_balance)
        .build();

    let transfer = alice.create_transfer(transfer_value);
    let multicall = alice.multicall_with_value(
        transfer_value,
        &[transfer.into(), alice.query_base_token_balance().into()],
    );

    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let output = test_call(&tx_sender, state_override, multicall)
        .await
        .unwrap();
    let call_results = Call3Result::parse(&output);
    assert_eq!(call_results.len(), 2);
    assert!(
        call_results[0].success && call_results[1].success,
        "{call_results:?}"
    );
    assert!(call_results[0].return_data.is_empty(), "{call_results:?}");

    let balance = call_results[1].as_u256();
    // The bootloader doesn't compute gas refunds in the call mode, so the equality is exact
    assert_eq!(balance, initial_balance - transfer_value);
}

#[tokio::test]
async fn eth_call_with_counter() {
    let alice = K256PrivateKey::random();
    let state_override = StateBuilder::default().with_counter_contract(42).build();

    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let output = test_call(
        &tx_sender,
        state_override.clone(),
        alice.query_counter_value(),
    )
    .await
    .unwrap();
    assert_eq!(decode_u256_output(&output), 42.into());

    let tx_as_call = alice.create_counter_tx(3.into(), false).into();
    let output = test_call(&tx_sender, state_override.clone(), tx_as_call)
        .await
        .unwrap();
    assert_eq!(decode_u256_output(&output), 45.into());

    let tx_as_call = alice.create_counter_tx(3.into(), true).into();
    let err = test_call(&tx_sender, state_override, tx_as_call)
        .await
        .unwrap_err();
    assert_matches!(
        err,
        SubmitTxError::ExecutionReverted(msg, _) if msg.contains("This method always reverts")
    );
}

#[tokio::test]
async fn eth_call_with_counter_transactions() {
    let alice = K256PrivateKey::random();
    let state_override = StateBuilder::default()
        .with_multicall3_contract()
        .with_counter_contract(0)
        .build();

    let multicall = alice.multicall_with_value(
        0.into(),
        &[
            alice.create_counter_tx(1.into(), false).into(),
            Call3Value::from(alice.create_counter_tx(2.into(), true)).allow_failure(),
            alice.query_counter_value().into(),
            alice.create_counter_tx(3.into(), false).into(),
        ],
    );
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let output = test_call(&tx_sender, state_override, multicall)
        .await
        .unwrap();
    let call_results = Call3Result::parse(&output);

    assert_eq!(
        call_results
            .iter()
            .map(|result| result.success)
            .collect::<Vec<_>>(),
        [true, false, true, true]
    );
    let counter_values: Vec<_> = call_results
        .iter()
        .filter_map(|result| {
            if !result.success {
                return None;
            }
            Some(decode_u256_output(&result.return_data).as_u32())
        })
        .collect();
    assert_eq!(counter_values, [1, 1, 4]);
}

#[tokio::test]
async fn eth_call_out_of_gas() {
    let alice = K256PrivateKey::random();
    let state_override = StateBuilder::default()
        .with_infinite_loop_contract()
        .build();

    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let tx_as_call = alice.create_infinite_loop_tx().into();
    let err = test_call(&tx_sender, state_override, tx_as_call)
        .await
        .unwrap_err();
    assert_matches!(err, SubmitTxError::ExecutionReverted(..));
}

#[tokio::test]
async fn eth_call_with_load_test_transactions() {
    let alice = K256PrivateKey::random();
    let state_override = StateBuilder::default().with_load_test_contract().build();

    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;

    // Deploys (naturally) don't work for calls, hence a separate set of test cases.
    let load_test_cases_for_call = [
        LoadnextContractExecutionParams {
            deploys: 0,
            ..LoadnextContractExecutionParams::default()
        },
        LoadnextContractExecutionParams {
            deploys: 0,
            recursive_calls: 20,
            ..LoadnextContractExecutionParams::default()
        },
        LoadnextContractExecutionParams {
            reads: 100,
            writes: 100,
            ..LoadnextContractExecutionParams::empty()
        },
    ];

    for tx_params in load_test_cases_for_call {
        println!("Executing {tx_params:?}");
        let tx_as_call = alice.create_load_test_tx(tx_params).into();
        test_call(&tx_sender, state_override.clone(), tx_as_call)
            .await
            .unwrap();
    }
}
