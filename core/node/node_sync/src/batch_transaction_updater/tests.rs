//! Tests for batch transaction updater.

use std::time::Duration;

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use zksync_dal::{Connection, Core};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{create_l1_batch, create_l2_block};
use zksync_types::{
    aggregated_operations::AggregatedActionType, block::L1BatchTreeData,
    commitment::L1BatchCommitmentArtifacts, eth_sender::EthTxFinalityStatus, web3::Log, Address,
    L1BatchNumber, L1BlockNumber, SLChainId, H256, U64,
};
use zksync_web3_decl::client::{MockClient, L1};

use super::*;
use crate::{
    batch_transaction_updater::l1_transaction_verifier::TransactionValidationError,
    metrics::L1BatchStage,
};

const MOCK_DIAMOND_PROXY_ADDRESS: zksync_types::H160 = Address::repeat_byte(0x42);

const SL_CHAIN_ID: SLChainId = SLChainId(1);

static INVALID_HASH: H256 = H256::repeat_byte(0xbe);

/// tx_type is 1 for commit, 2 for prove, 3 for execute
fn mock_block_number_for_batch_transaction(batch_number: L1BatchNumber, tx_type: u8) -> u32 {
    batch_number.0 * 10 + 100 + tx_type as u32
}

fn new_mock_eth_interface() -> Box<dyn EthInterface> {
    let contract = zksync_contracts::hyperchain_contract();
    Box::new(
        MockClient::builder(L1::default())
            .method("eth_getTransactionReceipt", move |tx_hash: H256| {
                // if "INVALID" transaction is requests we return a successful transaction, but without any logs
                if tx_hash == INVALID_HASH {
                    return Ok(Some(TransactionReceipt {
                        status: Some(U64::one()),
                        logs: vec![],
                        block_number: Some(U64([1u64])),
                        transaction_hash: tx_hash,
                        ..Default::default()
                    }));
                }

                // Extract the batch number from the tx hash
                // The batch number is stored in the last 4 bytes
                let bytes = tx_hash.as_bytes();
                let tx_type = bytes[0]; // 1 for commit, 2 for prove, 3 for execute

                // Extract batch number from the last 4 bytes
                let mut batch_number_bytes = [0u8; 4];
                batch_number_bytes.copy_from_slice(&bytes[28..32]);
                let batch_number = u32::from_be_bytes(batch_number_bytes);

                let block_number = Some(U64::from(mock_block_number_for_batch_transaction(
                    L1BatchNumber(batch_number),
                    tx_type,
                )));

                let topics: Vec<H256> = match tx_type {
                    1 => {
                        //BlockCommit (index_topic_1 uint256 blockNumber, index_topic_2 bytes32 blockHash, index_topic_3 bytes32 commitment)
                        let event = contract.event("BlockCommit").unwrap();
                        vec![
                            event.signature(),
                            H256::from_low_u64_be(batch_number.into()),
                            H256::zero(),
                            H256::zero(),
                        ]
                    }
                    2 => {
                        // BlocksVerification (index_topic_1 uint256 previousLastVerifiedBlock, index_topic_2 uint256 currentLastVerifiedBlock
                        let event = contract.event("BlocksVerification").unwrap();
                        vec![
                            event.signature(),
                            H256::from_low_u64_be((batch_number - 1).into()),
                            H256::from_low_u64_be(batch_number.into()),
                        ]
                    }
                    3 => {
                        // BlockExecution (index_topic_1 uint256 blockNumber, index_topic_2 bytes32 blockHash, index_topic_3 bytes32 commitment)
                        let event = contract.event("BlockExecution").unwrap();
                        vec![
                            event.signature(),
                            H256::from_low_u64_be(batch_number.into()),
                            H256::zero(),
                            H256::zero(),
                        ]
                    }
                    _ => return Ok(None),
                };

                // Create a receipt with status 1 (success)
                let receipt = TransactionReceipt {
                    status: Some(U64::one()),
                    block_number,
                    transaction_hash: tx_hash,
                    logs: vec![Log {
                        address: MOCK_DIAMOND_PROXY_ADDRESS,
                        topics,
                        data: vec![].into(),
                        block_hash: None,
                        block_number,
                        transaction_hash: Some(tx_hash),
                        transaction_index: None,
                        log_index: None,
                        transaction_log_index: None,
                        log_type: Some("Regular".to_string()),
                        removed: None,
                        block_timestamp: None,
                    }],
                    ..Default::default()
                };

                Ok(Some(receipt))
            })
            .build(),
    )
}

async fn seal_l1_batch(storage: &mut Connection<'_, Core>, number: L1BatchNumber) {
    let mut storage = storage.start_transaction().await.unwrap();
    // Insert a mock L2 block so that `get_block_details()` will return values.
    let l2_block = create_l2_block(number.0);
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block)
        .await
        .unwrap();

    let l1_batch = create_l1_batch(number.0);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&l1_batch)
        .await
        .unwrap();

    storage
        .blocks_dal()
        .save_l1_batch_tree_data(
            number,
            &L1BatchTreeData {
                hash: H256::zero(),
                rollup_last_leaf_index: 0,
            },
        )
        .await
        .unwrap();

    storage
        .blocks_dal()
        .save_l1_batch_commitment_artifacts(number, &L1BatchCommitmentArtifacts::default())
        .await
        .unwrap();

    storage
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(number)
        .await
        .unwrap();
    storage.commit().await.unwrap();
}

/// Helper function to insert a transaction for a specific action type.
async fn insert_tx(
    storage: &mut Connection<'_, Core>,
    batch_number: L1BatchNumber,
    action_type: AggregatedActionType,
) -> anyhow::Result<()> {
    let tx_hash = create_tx_hash(action_type, batch_number.0);
    storage
        .eth_sender_dal()
        .insert_pending_received_eth_tx(batch_number, action_type, tx_hash, Some(SL_CHAIN_ID))
        .await?;
    Ok(())
}

/// Helper function to insert an invalid transaction that will fail validation
async fn insert_invalid_tx(
    storage: &mut Connection<'_, Core>,
    batch_number: L1BatchNumber,
    action_type: AggregatedActionType,
) -> anyhow::Result<()> {
    storage
        .eth_sender_dal()
        .insert_pending_received_eth_tx(batch_number, action_type, INVALID_HASH, Some(SL_CHAIN_ID))
        .await?;
    Ok(())
}

/// Helper function to set up the test environment
/// Returns a tuple of (pool, storage, updater, batch_number, genesis_params)
async fn setup_test_environment() -> anyhow::Result<(
    zksync_dal::ConnectionPool<Core>,
    Connection<'static, Core>,
    BatchTransactionUpdater,
    L1BatchNumber,
    GenesisParams,
)> {
    let pool = zksync_dal::ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await?;

    // Create genesis batch
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut storage, &genesis_params).await?;

    // Create a batch and seal it
    let batch_number = L1BatchNumber(1);
    seal_l1_batch(&mut storage, batch_number).await;

    // Create BatchTransactionUpdater
    let updater = BatchTransactionUpdater::new(
        new_mock_eth_interface(),
        MOCK_DIAMOND_PROXY_ADDRESS,
        pool.clone(),
        Duration::from_secs(1),
        NonZeroU64::new(10).unwrap(), // processing_batch_size
    );

    Ok((pool, storage, updater, batch_number, genesis_params))
}

/// Insert transactions into the database based on the stage
/// This simulates what BatchStatusUpdater would do
async fn insert_batch_transactions(
    storage: &mut Connection<'_, Core>,
    batch_number: L1BatchNumber,
    stage: L1BatchStage,
) -> anyhow::Result<()> {
    // For each stage, insert the appropriate transactions
    if stage >= L1BatchStage::Committed {
        // Insert commit transaction
        insert_tx(storage, batch_number, AggregatedActionType::Commit).await?;
    }

    if stage >= L1BatchStage::Proven {
        // Insert prove transaction
        insert_tx(
            storage,
            batch_number,
            AggregatedActionType::PublishProofOnchain,
        )
        .await?;
    }

    if stage >= L1BatchStage::Executed {
        // Insert execute transaction
        insert_tx(storage, batch_number, AggregatedActionType::Execute).await?;
    }

    Ok(())
}
/// Verify that transaction statuses in the database match the expected values
async fn verify_transaction_statuses_separate(
    storage: &mut Connection<'_, Core>,
    batch_number: L1BatchNumber,
    expected_commit_status: Option<EthTxFinalityStatus>,
    expected_prove_status: Option<EthTxFinalityStatus>,
    expected_execute_status: Option<EthTxFinalityStatus>,
) -> anyhow::Result<()> {
    // Get batch details from the database
    let batch_details = storage
        .blocks_web3_dal()
        .get_l1_batch_details(batch_number)
        .await?
        .expect("Batch should exist");

    assert_eq!(
        batch_details.base.commit_tx_finality, expected_commit_status,
        "Commit transaction finality status mismatch"
    );
    assert_eq!(
        batch_details.base.prove_tx_finality, expected_prove_status,
        "Prove transaction finality status mismatch"
    );
    assert_eq!(
        batch_details.base.execute_tx_finality, expected_execute_status,
        "Execute transaction finality status mismatch"
    );

    Ok(())
}

/// Verify that transaction statuses in the database match the expected one value
async fn verify_transaction_statuses(
    storage: &mut Connection<'_, Core>,
    batch_number: L1BatchNumber,
    stage: L1BatchStage,
    expected_finality_status: EthTxFinalityStatus,
) -> anyhow::Result<()> {
    let expected_finality_status = if expected_finality_status == EthTxFinalityStatus::Pending {
        None
    } else {
        Some(expected_finality_status)
    };
    verify_transaction_statuses_separate(
        storage,
        batch_number,
        if stage >= L1BatchStage::Committed {
            expected_finality_status
        } else {
            None
        },
        if stage >= L1BatchStage::Proven {
            expected_finality_status
        } else {
            None
        },
        if stage >= L1BatchStage::Executed {
            expected_finality_status
        } else {
            None
        },
    )
    .await
}

/// Helper function to create transaction hash
/// The first byte is the transaction type (1 for commit, 2 for prove, 3 for execute)
/// The last 4 bytes are the batch number
fn create_tx_hash(tx_type: AggregatedActionType, batch_number: u32) -> H256 {
    let mut h = [0u8; 32];
    h[0] = match tx_type {
        AggregatedActionType::Commit => 1,
        AggregatedActionType::PublishProofOnchain => 2,
        AggregatedActionType::Execute => 3,
    };
    h[28..].copy_from_slice(&batch_number.to_be_bytes());
    H256::from(h)
}

#[test_casing(9, Product(([L1BatchStage::Committed, L1BatchStage::Proven, L1BatchStage::Executed], [EthTxFinalityStatus::Pending, EthTxFinalityStatus::FastFinalized, EthTxFinalityStatus::Finalized])))]
#[tokio::test]
async fn normal_operation_1_batch(
    stage: L1BatchStage,
    finality_status: EthTxFinalityStatus,
) -> anyhow::Result<()> {
    // Set up test environment
    let (_pool, mut storage, updater, batch_number, _genesis_params) =
        setup_test_environment().await?;

    let transactions_l1_block_number =
        L1BlockNumber(mock_block_number_for_batch_transaction(batch_number, 3));

    // Insert transactions into the database based on the stage
    // This simulates what BatchStatusUpdater would do
    insert_batch_transactions(&mut storage, batch_number, stage).await?;

    // Run the updater once
    let l1_block_numbers = match finality_status {
        EthTxFinalityStatus::Pending => L1BlockNumbers {
            finalized: L1BlockNumber(10),
            fast_finality: L1BlockNumber(10),
            latest: transactions_l1_block_number,
        },
        EthTxFinalityStatus::FastFinalized => L1BlockNumbers {
            finalized: L1BlockNumber(10),
            fast_finality: transactions_l1_block_number,
            latest: transactions_l1_block_number,
        },
        EthTxFinalityStatus::Finalized => L1BlockNumbers {
            finalized: transactions_l1_block_number,
            fast_finality: transactions_l1_block_number,
            latest: transactions_l1_block_number,
        },
    };

    // Update transaction statuses

    let updated_count = updater
        .loop_iteration(SL_CHAIN_ID, l1_block_numbers)
        .await?;

    // Verify the transaction statuses in the database
    verify_transaction_statuses(&mut storage, batch_number, stage, finality_status).await?;

    // verify expected update count
    if finality_status == EthTxFinalityStatus::Pending {
        assert_eq!(updated_count, 0);
    } else {
        match stage {
            L1BatchStage::Committed => {
                // For committed stage, we should have updated 1 transaction
                assert_eq!(updated_count, 1);
            }
            L1BatchStage::Proven => {
                // For proven stage, we should have updated 2 transactions (commit + prove)
                assert_eq!(updated_count, 2);
            }
            L1BatchStage::Executed => {
                // For executed stage, we should have updated 3 transactions (commit + prove + execute)
                assert_eq!(updated_count, 3);
            }
            _ => unreachable!("Test only runs with Committed, Proven, or Executed stages"),
        }
    }

    // Test second update does not change anything
    let updated_count = updater
        .loop_iteration(SL_CHAIN_ID, l1_block_numbers)
        .await?;
    assert_eq!(updated_count, 0);
    verify_transaction_statuses(&mut storage, batch_number, stage, finality_status).await?;

    Ok(())
}

#[tokio::test]
async fn test_new_transactions_between_updates_with_finality_change() -> anyhow::Result<()> {
    // Set up test environment
    let (_pool, mut storage, updater, batch_number, _genesis_params) =
        setup_test_environment().await?;

    // Start with commit tx online
    insert_tx(&mut storage, batch_number, AggregatedActionType::Commit).await?;

    // STAGE 1: Initial update with pending transactions
    // ------------------------------------------------------------
    // First update - should not change anything as transactions are pending
    let block_commit = mock_block_number_for_batch_transaction(batch_number, 1);
    let block_prove = mock_block_number_for_batch_transaction(batch_number, 2);
    let block_execute = mock_block_number_for_batch_transaction(batch_number, 3);

    let updated_count = updater
        .loop_iteration(
            SL_CHAIN_ID,
            L1BlockNumbers {
                finalized: L1BlockNumber(30),
                fast_finality: L1BlockNumber(30),
                latest: L1BlockNumber(block_execute),
            },
        )
        .await?;

    // Verify no updates occurred
    assert_eq!(updated_count, 0);
    verify_transaction_statuses_separate(&mut storage, batch_number, None, None, None).await?;

    // STAGE 2: Add prove transaction and update with partial finality
    // ------------------------------------------------------------
    // Add Prove transaction (progressing to Proven stage)
    insert_tx(
        &mut storage,
        batch_number,
        AggregatedActionType::PublishProofOnchain,
    )
    .await?;

    // Update with blocks that won't finalize any transactions
    let updated_count = updater
        .loop_iteration(
            SL_CHAIN_ID,
            L1BlockNumbers {
                finalized: L1BlockNumber(50),
                fast_finality: L1BlockNumber(70),
                latest: L1BlockNumber(block_prove),
            },
        )
        .await?;
    assert_eq!(updated_count, 0);
    verify_transaction_statuses_separate(&mut storage, batch_number, None, None, None).await?;

    // Update with blocks that will fast-finalize only the commit transaction
    let updated_count = updater
        .loop_iteration(
            SL_CHAIN_ID,
            L1BlockNumbers {
                finalized: L1BlockNumber(50),
                fast_finality: L1BlockNumber(block_commit),
                latest: L1BlockNumber(block_prove),
            },
        )
        .await?;
    assert_eq!(updated_count, 1); // Only commit transaction should be updated

    verify_transaction_statuses_separate(
        &mut storage,
        batch_number,
        Some(EthTxFinalityStatus::FastFinalized),
        None,
        None,
    )
    .await?;

    // STAGE 3: Add execute transaction and update with mixed finality
    // ------------------------------------------------------------
    // Add execute transaction for current batch and create next batch with all transactions
    insert_tx(&mut storage, batch_number, AggregatedActionType::Execute).await?;
    seal_l1_batch(&mut storage, batch_number + 1).await;
    insert_batch_transactions(&mut storage, batch_number + 1, L1BatchStage::Executed).await?;

    // Update with blocks that will finalize most transactions
    let block_next_prove = mock_block_number_for_batch_transaction(batch_number + 1, 2);
    let block_next_execute = mock_block_number_for_batch_transaction(batch_number + 1, 3);

    let updated_count = updater
        .loop_iteration(
            SL_CHAIN_ID,
            L1BlockNumbers {
                finalized: L1BlockNumber(block_next_prove), // Finalized up to next batch's prove
                fast_finality: L1BlockNumber(block_next_execute), // Fast-finalized up to next batch's execute
                latest: L1BlockNumber(block_next_execute), // Latest is at next batch's execute
            },
        )
        .await?;

    // All transactions get updated, all but last execute to finalized
    assert_eq!(updated_count, 6);

    // Verify first batch is fully finalized
    verify_transaction_statuses_separate(
        &mut storage,
        batch_number,
        Some(EthTxFinalityStatus::Finalized),
        Some(EthTxFinalityStatus::Finalized),
        Some(EthTxFinalityStatus::Finalized),
    )
    .await?;

    // Verify second batch has mixed finality status
    verify_transaction_statuses_separate(
        &mut storage,
        batch_number + 1,
        Some(EthTxFinalityStatus::Finalized),
        Some(EthTxFinalityStatus::Finalized),
        Some(EthTxFinalityStatus::FastFinalized),
    )
    .await?;

    // STAGE 4: Final update to fully finalize all transactions
    // ------------------------------------------------------------
    // Update with blocks that will finalize all transactions
    let final_l1_block_numbers = L1BlockNumbers {
        finalized: L1BlockNumber(block_next_execute),
        fast_finality: L1BlockNumber(block_next_execute),
        latest: L1BlockNumber(block_next_execute),
    };

    // Final update - should update last execute to finalized
    let updated_count = updater
        .loop_iteration(SL_CHAIN_ID, final_l1_block_numbers)
        .await?;
    assert_eq!(updated_count, 1);

    // Verify second batch is now fully finalized
    verify_transaction_statuses_separate(
        &mut storage,
        batch_number + 1,
        Some(EthTxFinalityStatus::Finalized),
        Some(EthTxFinalityStatus::Finalized),
        Some(EthTxFinalityStatus::Finalized),
    )
    .await?;

    Ok(())
}

#[test_casing(3, [AggregatedActionType::Commit, AggregatedActionType::PublishProofOnchain, AggregatedActionType::Execute])]
#[tokio::test]
async fn test_invalid_transaction_handling(
    invalid_tx_type: AggregatedActionType,
) -> anyhow::Result<()> {
    // Set up test environment
    let (_pool, mut storage, updater, batch_number, _genesis_params) =
        setup_test_environment().await?;

    // Insert all three transaction types, but make the specified one invalid
    if invalid_tx_type != AggregatedActionType::Commit {
        insert_tx(&mut storage, batch_number, AggregatedActionType::Commit).await?;
    } else {
        insert_invalid_tx(&mut storage, batch_number, AggregatedActionType::Commit).await?;
    }

    if invalid_tx_type != AggregatedActionType::PublishProofOnchain {
        insert_tx(
            &mut storage,
            batch_number,
            AggregatedActionType::PublishProofOnchain,
        )
        .await?;
    } else {
        insert_invalid_tx(
            &mut storage,
            batch_number,
            AggregatedActionType::PublishProofOnchain,
        )
        .await?;
    }

    if invalid_tx_type != AggregatedActionType::Execute {
        insert_tx(&mut storage, batch_number, AggregatedActionType::Execute).await?;
    } else {
        insert_invalid_tx(&mut storage, batch_number, AggregatedActionType::Execute).await?;
    }

    // Update with blocks that would finalize all transactions
    let block_execute = mock_block_number_for_batch_transaction(batch_number, 3);

    // Should fail as the transaction is invalid
    let err = updater
        .loop_iteration(
            SL_CHAIN_ID,
            L1BlockNumbers {
                finalized: L1BlockNumber(block_execute),
                fast_finality: L1BlockNumber(block_execute),
                latest: L1BlockNumber(block_execute),
            },
        )
        .await
        .unwrap_err();

    // Failure should be due to invalid transaction
    assert_matches!(
        err.downcast_ref::<TransactionValidationError>()
            .expect("Unexpected error type"),
        TransactionValidationError::BatchTransactionInvalid { .. }
    );

    Ok(())
}
