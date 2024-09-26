//! Tests for the VM execution sandbox.

use std::collections::HashMap;

use assert_matches::assert_matches;
use test_casing::test_casing;
use zksync_dal::ConnectionPool;
use zksync_multivm::{interface::ExecutionResult, utils::derive_base_fee_and_gas_per_pubdata};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{create_l2_block, prepare_recovery_snapshot};
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    api::state_override::{OverrideAccount, StateOverride},
    fee::Fee,
    fee_model::BatchFeeInput,
    l2::L2Tx,
    transaction_request::PaymasterParams,
    Address, K256PrivateKey, L2ChainId, Nonce, ProtocolVersionId, Transaction, U256,
};

use super::*;
use crate::{execution_sandbox::execute::SandboxExecutor, tx_sender::SandboxExecutorOptions};

#[tokio::test]
async fn creating_block_args() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    let l2_block = create_l2_block(1);
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block)
        .await
        .unwrap();

    let pending_block_args = BlockArgs::pending(&mut storage).await.unwrap();
    assert_eq!(
        pending_block_args.block_id,
        api::BlockId::Number(api::BlockNumber::Pending)
    );
    assert_eq!(pending_block_args.resolved_block_number(), L2BlockNumber(2));
    assert_eq!(pending_block_args.inner.l1_batch_timestamp(), None);
    assert!(pending_block_args.is_pending());

    let start_info = BlockStartInfo::new(&mut storage, Duration::MAX)
        .await
        .unwrap();
    assert_eq!(
        start_info.first_l2_block(&mut storage).await.unwrap(),
        L2BlockNumber(0)
    );
    assert_eq!(
        start_info.first_l1_batch(&mut storage).await.unwrap(),
        L1BatchNumber(0)
    );

    let latest_block = api::BlockId::Number(api::BlockNumber::Latest);
    let latest_block_args = BlockArgs::new(&mut storage, latest_block, &start_info)
        .await
        .unwrap();
    assert_eq!(latest_block_args.block_id, latest_block);
    assert_eq!(latest_block_args.resolved_block_number(), L2BlockNumber(1));
    assert_eq!(
        latest_block_args.inner.l1_batch_timestamp(),
        Some(l2_block.timestamp)
    );

    let earliest_block = api::BlockId::Number(api::BlockNumber::Earliest);
    let earliest_block_args = BlockArgs::new(&mut storage, earliest_block, &start_info)
        .await
        .unwrap();
    assert_eq!(earliest_block_args.block_id, earliest_block);
    assert_eq!(
        earliest_block_args.resolved_block_number(),
        L2BlockNumber(0)
    );
    assert_eq!(earliest_block_args.inner.l1_batch_timestamp(), Some(0));

    let missing_block = api::BlockId::Number(100.into());
    let err = BlockArgs::new(&mut storage, missing_block, &start_info)
        .await
        .unwrap_err();
    assert_matches!(err, BlockArgsError::Missing);
}

#[tokio::test]
async fn creating_block_args_after_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;

    let pending_block_args = BlockArgs::pending(&mut storage).await.unwrap();
    assert_eq!(
        pending_block_args.block_id,
        api::BlockId::Number(api::BlockNumber::Pending)
    );
    assert_eq!(
        pending_block_args.resolved_block_number(),
        snapshot_recovery.l2_block_number + 1
    );
    assert!(pending_block_args.is_pending());

    let start_info = BlockStartInfo::new(&mut storage, Duration::MAX)
        .await
        .unwrap();
    assert_eq!(
        start_info.first_l2_block(&mut storage).await.unwrap(),
        snapshot_recovery.l2_block_number + 1
    );
    assert_eq!(
        start_info.first_l1_batch(&mut storage).await.unwrap(),
        snapshot_recovery.l1_batch_number + 1
    );

    let latest_block = api::BlockId::Number(api::BlockNumber::Latest);
    let err = BlockArgs::new(&mut storage, latest_block, &start_info)
        .await
        .unwrap_err();
    assert_matches!(err, BlockArgsError::Missing);

    let pruned_blocks = [
        api::BlockNumber::Earliest,
        0.into(),
        snapshot_recovery.l2_block_number.0.into(),
    ];
    for pruned_block in pruned_blocks {
        let pruned_block = api::BlockId::Number(pruned_block);
        let err = BlockArgs::new(&mut storage, pruned_block, &start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Pruned(_));
    }

    let missing_blocks = [
        api::BlockNumber::from(snapshot_recovery.l2_block_number.0 + 2),
        100.into(),
    ];
    for missing_block in missing_blocks {
        let missing_block = api::BlockId::Number(missing_block);
        let err = BlockArgs::new(&mut storage, missing_block, &start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Missing);
    }

    let l2_block = create_l2_block(snapshot_recovery.l2_block_number.0 + 1);
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block)
        .await
        .unwrap();

    let latest_block_args = BlockArgs::new(&mut storage, latest_block, &start_info)
        .await
        .unwrap();
    assert_eq!(latest_block_args.block_id, latest_block);
    assert_eq!(latest_block_args.resolved_block_number(), l2_block.number);
    assert_eq!(
        latest_block_args.inner.l1_batch_timestamp(),
        Some(l2_block.timestamp)
    );

    for pruned_block in pruned_blocks {
        let pruned_block = api::BlockId::Number(pruned_block);
        let err = BlockArgs::new(&mut storage, pruned_block, &start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Pruned(_));
    }
    for missing_block in missing_blocks {
        let missing_block = api::BlockId::Number(missing_block);
        let err = BlockArgs::new(&mut storage, missing_block, &start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Missing);
    }
}

#[tokio::test]
async fn estimating_gas() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut connection = pool.connection().await.unwrap();
    insert_genesis_batch(&mut connection, &GenesisParams::mock())
        .await
        .unwrap();

    let block_args = BlockArgs::pending(&mut connection).await.unwrap();
    test_instantiating_vm(connection, block_args).await;

    let mut connection = pool.connection().await.unwrap();
    let start_info = BlockStartInfo::new(&mut connection, Duration::MAX)
        .await
        .unwrap();
    let block_args = BlockArgs::new(&mut connection, api::BlockId::Number(0.into()), &start_info)
        .await
        .unwrap();
    test_instantiating_vm(connection, block_args).await;
}

async fn test_instantiating_vm(connection: Connection<'static, Core>, block_args: BlockArgs) {
    let executor = SandboxExecutor::real(
        SandboxExecutorOptions::mock().await,
        PostgresStorageCaches::new(1, 1),
        usize::MAX,
    );

    let fee_input = BatchFeeInput::l1_pegged(55, 555);
    let (base_fee, gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());
    let tx = Transaction::from(create_transfer(base_fee, gas_per_pubdata));

    let (limiter, _) = VmConcurrencyLimiter::new(1);
    let vm_permit = limiter.acquire().await.unwrap();
    let action = SandboxAction::GasEstimation {
        fee_input,
        base_fee,
        tx,
    };
    let output = executor
        .execute_in_sandbox(vm_permit, connection, action, &block_args, None)
        .await
        .unwrap();

    assert!(output.are_published_bytecodes_ok);
    let tx_result = output.vm;
    assert!(!tx_result.result.is_failed(), "{tx_result:#?}");
}

fn create_transfer(fee_per_gas: u64, gas_per_pubdata: u64) -> L2Tx {
    let fee = Fee {
        gas_limit: 200_000.into(),
        max_fee_per_gas: fee_per_gas.into(),
        max_priority_fee_per_gas: 0_u64.into(),
        gas_per_pubdata_limit: gas_per_pubdata.into(),
    };
    L2Tx::new_signed(
        Some(Address::random()),
        vec![],
        Nonce(0),
        fee,
        U256::zero(),
        L2ChainId::default(),
        &K256PrivateKey::random(),
        vec![],
        PaymasterParams::default(),
    )
    .unwrap()
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn validating_transaction(set_balance: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut connection = pool.connection().await.unwrap();
    insert_genesis_batch(&mut connection, &GenesisParams::mock())
        .await
        .unwrap();

    let block_args = BlockArgs::pending(&mut connection).await.unwrap();

    let executor = SandboxExecutor::real(
        SandboxExecutorOptions::mock().await,
        PostgresStorageCaches::new(1, 1),
        usize::MAX,
    );

    let fee_input = BatchFeeInput::l1_pegged(55, 555);
    let (base_fee, gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());
    let tx = create_transfer(base_fee, gas_per_pubdata);

    let (limiter, _) = VmConcurrencyLimiter::new(1);
    let vm_permit = limiter.acquire().await.unwrap();
    let state_override = if set_balance {
        let account_override = OverrideAccount {
            balance: Some(U256::from(1) << 128),
            ..OverrideAccount::default()
        };
        StateOverride::new(HashMap::from([(tx.initiator_account(), account_override)]))
    } else {
        StateOverride::default()
    };

    let result = executor
        .execute_in_sandbox(
            vm_permit,
            connection,
            SandboxAction::Execution { tx, fee_input },
            &block_args,
            Some(state_override),
        )
        .await
        .unwrap();

    let result = result.vm.result;
    if set_balance {
        assert_matches!(result, ExecutionResult::Success { .. });
    } else {
        assert_matches!(
            result,
            ExecutionResult::Halt { reason } if reason.to_string().contains("Not enough balance")
        );
    }
}
