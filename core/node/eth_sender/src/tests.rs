use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_l1_contract_interface::i_executor::methods::ExecuteBatches;
use zksync_node_test_utils::create_l1_batch;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    block::L1BatchHeader,
    commitment::{
        L1BatchCommitmentMode, L1BatchMetaParameters, L1BatchMetadata, L1BatchWithMetadata,
    },
    ethabi::Token,
    helpers::unix_timestamp_ms,
    web3::contract::Error,
    ProtocolVersionId, H256,
};

use crate::{
    abstract_l1_interface::OperatorType,
    aggregated_operations::AggregatedOperation,
    tester::{EthSenderTester, TestL1Batch},
    EthSenderError,
};

fn get_dummy_operation(number: u32) -> AggregatedOperation {
    AggregatedOperation::Execute(ExecuteBatches {
        l1_batches: vec![L1BatchWithMetadata {
            header: create_l1_batch(number),
            metadata: default_l1_batch_metadata(),
            raw_published_factory_deps: Vec::new(),
        }],
    })
}

const COMMITMENT_MODES: [L1BatchCommitmentMode; 2] = [
    L1BatchCommitmentMode::Rollup,
    L1BatchCommitmentMode::Validium,
];

pub(crate) fn mock_multicall_response() -> Token {
    Token::Array(vec![
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![1u8; 32])]),
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![2u8; 32])]),
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![3u8; 96])]),
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![4u8; 32])]),
        Token::Tuple(vec![
            Token::Bool(true),
            Token::Bytes(
                H256::from_low_u64_be(ProtocolVersionId::default() as u64)
                    .0
                    .to_vec(),
            ),
        ]),
    ])
}

pub(crate) fn l1_batch_with_metadata(header: L1BatchHeader) -> L1BatchWithMetadata {
    L1BatchWithMetadata {
        header,
        metadata: default_l1_batch_metadata(),
        raw_published_factory_deps: vec![],
    }
}

pub(crate) fn default_l1_batch_metadata() -> L1BatchMetadata {
    L1BatchMetadata {
        root_hash: H256::default(),
        rollup_last_leaf_index: 0,
        initial_writes_compressed: Some(vec![]),
        repeated_writes_compressed: Some(vec![]),
        commitment: H256::default(),
        l2_l1_merkle_root: H256::default(),
        block_meta_params: L1BatchMetaParameters {
            zkporter_is_available: false,
            bootloader_code_hash: H256::default(),
            default_aa_code_hash: H256::default(),
            protocol_version: Some(ProtocolVersionId::default()),
        },
        aux_data_hash: H256::default(),
        meta_parameters_hash: H256::default(),
        pass_through_data_hash: H256::default(),
        events_queue_commitment: Some(H256::zero()),
        bootloader_initial_content_commitment: Some(H256::zero()),
        state_diffs_compressed: vec![],
    }
}

// Tests that we send multiple transactions and confirm them all in one iteration.
#[test_casing(4, Product(([false, true], COMMITMENT_MODES)))]
#[test_log::test(tokio::test)]
async fn confirm_many(
    aggregator_operate_4844_mode: bool,
    commitment_mode: L1BatchCommitmentMode,
) -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = EthSenderTester::new(
        connection_pool.clone(),
        vec![10; 100],
        false,
        aggregator_operate_4844_mode,
        commitment_mode,
    )
    .await;

    let mut l1_batches = vec![];

    let _genesis_batch = TestL1Batch::sealed(&mut tester).await;
    for _ in 1..6 {
        let l1_batch = TestL1Batch::sealed(&mut tester).await;
        l1_batch.save_commit_tx(&mut tester).await;
        l1_batches.push(l1_batch);
    }
    tester.run_eth_sender_tx_manager_iteration().await;

    // check that we sent something
    tester.assert_just_sent_tx_count_equals(5).await;

    tester.run_eth_sender_tx_manager_iteration().await;

    for l1_batch in l1_batches {
        l1_batch.execute_commit_tx(&mut tester).await;
    }

    tester.run_eth_sender_tx_manager_iteration().await;

    // check that all transactions are marked as accepted
    tester.assert_inflight_txs_count_equals(0).await;

    // also check that we didn't try to resend it
    tester.assert_just_sent_tx_count_equals(0).await;

    Ok(())
}

// Tests that we resend first un-mined transaction every block with an increased gas price.
#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn resend_each_block(commitment_mode: L1BatchCommitmentMode) -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = EthSenderTester::new(
        connection_pool.clone(),
        vec![7, 6, 5, 5, 5, 2, 1],
        false,
        true,
        commitment_mode,
    )
    .await;

    // after this, median should be 6
    tester.gateway.advance_block_number(3);
    tester.gas_adjuster.keep_updated().await?;

    TestL1Batch::sealed(&mut tester).await;

    let block = tester.get_block_numbers().await.latest;

    let tx = tester
        .aggregator
        .save_eth_tx(
            &mut tester.conn.connection().await.unwrap(),
            &get_dummy_operation(0),
            false,
            false,
        )
        .await?;

    let hash = tester
        .manager
        .send_eth_tx(&mut tester.conn.connection().await.unwrap(), &tx, 0, block)
        .await?;

    // check that we sent something and stored it in the db
    assert_eq!(tester.gateway.sent_tx_count(), 1);
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs(
                tester.manager.operator_address(OperatorType::NonBlob),
                false
            )
            .await
            .unwrap()
            .len(),
        1
    );

    let sent_tx = tester
        .manager
        .l1_interface()
        .get_tx(hash, OperatorType::NonBlob)
        .await
        .unwrap()
        .expect("no transaction");
    assert_eq!(sent_tx.hash, hash);
    assert_eq!(sent_tx.nonce, 0.into());
    assert_eq!(
        sent_tx.max_fee_per_gas.unwrap() - sent_tx.max_priority_fee_per_gas.unwrap(),
        18.into() // `6 * 3 * 2^0`
    );

    // now, median is 5
    tester.gateway.advance_block_number(2);
    tester.gas_adjuster.keep_updated().await?;
    let block_numbers = tester.get_block_numbers().await;

    let (to_resend, _) = tester
        .manager
        .monitor_inflight_transactions_single_operator(
            &mut tester.conn.connection().await.unwrap(),
            block_numbers,
            OperatorType::NonBlob,
        )
        .await?
        .unwrap();

    let resent_hash = tester
        .manager
        .send_eth_tx(
            &mut tester.conn.connection().await.unwrap(),
            &to_resend,
            1,
            block_numbers.latest,
        )
        .await?;

    // check that transaction has been resent
    assert_eq!(tester.gateway.sent_tx_count(), 2);
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs(
                tester.manager.operator_address(OperatorType::NonBlob),
                false
            )
            .await
            .unwrap()
            .len(),
        1
    );

    let resent_tx = tester
        .manager
        .l1_interface()
        .get_tx(resent_hash, OperatorType::NonBlob)
        .await
        .unwrap()
        .expect("no transaction");
    assert_eq!(resent_tx.nonce, 0.into());
    assert_eq!(
        resent_tx.max_fee_per_gas.unwrap() - resent_tx.max_priority_fee_per_gas.unwrap(),
        30.into() // `5 * 3 * 2^1`
    );

    Ok(())
}

// Tests that if transaction was mined, but not enough blocks has been mined since,
// we won't mark it as confirmed but also won't resend it.
#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn dont_resend_already_mined(commitment_mode: L1BatchCommitmentMode) -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = EthSenderTester::new(
        connection_pool.clone(),
        vec![100; 100],
        false,
        true,
        commitment_mode,
    )
    .await;

    let _genesis_batch = TestL1Batch::sealed(&mut tester).await;
    let l1_batch = TestL1Batch::sealed(&mut tester).await;
    l1_batch.save_commit_tx(&mut tester).await;

    tester.run_eth_sender_tx_manager_iteration().await;

    // check that we sent something and stored it in the db
    tester.assert_just_sent_tx_count_equals(1).await;
    tester.assert_inflight_txs_count_equals(1).await;

    // mine the transaction but don't have enough confirmations yet
    tester
        .execute_tx(
            l1_batch.number,
            AggregatedActionType::Commit,
            true,
            // we use -2 as running eth_sender iteration implicitly advances block number by 1
            EthSenderTester::WAIT_CONFIRMATIONS - 2,
        )
        .await;
    tester.run_eth_sender_tx_manager_iteration().await;

    // check that transaction is still considered in-flight
    tester.assert_inflight_txs_count_equals(1).await;

    // also check that we didn't try to resend it
    tester.assert_just_sent_tx_count_equals(0).await;

    Ok(())
}

#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn three_scenarios(commitment_mode: L1BatchCommitmentMode) -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = EthSenderTester::new(
        connection_pool.clone(),
        vec![100; 100],
        false,
        true,
        commitment_mode,
    )
    .await;

    let _genesis_batch = TestL1Batch::sealed(&mut tester).await;

    let first_batch = TestL1Batch::sealed(&mut tester).await;
    let second_batch = TestL1Batch::sealed(&mut tester).await;
    let third_batch = TestL1Batch::sealed(&mut tester).await;
    let fourth_batch = TestL1Batch::sealed(&mut tester).await;

    first_batch.save_commit_tx(&mut tester).await;
    second_batch.save_commit_tx(&mut tester).await;
    third_batch.save_commit_tx(&mut tester).await;
    fourth_batch.save_commit_tx(&mut tester).await;

    tester.run_eth_sender_tx_manager_iteration().await;
    // we should have sent transactions for all batches for the first time
    tester.assert_just_sent_tx_count_equals(4).await;

    first_batch.execute_commit_tx(&mut tester).await;
    second_batch.execute_commit_tx(&mut tester).await;

    tester.run_eth_sender_tx_manager_iteration().await;
    // check that last 2 transactions are still considered in-flight
    tester.assert_inflight_txs_count_equals(2).await;

    //We should have resent only first not-mined transaction
    third_batch.assert_commit_tx_just_sent(&mut tester).await;
    tester.assert_just_sent_tx_count_equals(1).await;

    Ok(())
}

#[should_panic(expected = "We can't operate after tx fail")]
#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn failed_eth_tx(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = EthSenderTester::new(
        connection_pool.clone(),
        vec![100; 100],
        false,
        true,
        commitment_mode,
    )
    .await;

    let _genesis_batch = TestL1Batch::sealed(&mut tester).await;
    let first_batch = TestL1Batch::sealed(&mut tester).await;

    first_batch.save_commit_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;

    first_batch.fail_commit_tx(&mut tester).await;

    tester.run_eth_sender_tx_manager_iteration().await;
}

#[test_log::test(tokio::test)]
async fn blob_transactions_are_resent_independently_of_non_blob_txs() {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        true,
        L1BatchCommitmentMode::Rollup,
    )
    .await;

    let _genesis_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let first_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let second_l1_batch = TestL1Batch::sealed(&mut tester).await;

    first_l1_batch.save_commit_tx(&mut tester).await;
    second_l1_batch.save_commit_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;
    // first iteration sends two commit txs for the first time
    tester.assert_just_sent_tx_count_equals(2).await;

    first_l1_batch.save_prove_tx(&mut tester).await;
    first_l1_batch.execute_commit_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;
    // second iteration sends first_batch prove tx and resends second_batch commit tx
    tester.assert_just_sent_tx_count_equals(2).await;

    tester.run_eth_sender_tx_manager_iteration().await;
    // we should resend both of those transactions here as they use different operators
    tester.assert_just_sent_tx_count_equals(2).await;
}

#[test_log::test(tokio::test)]
async fn transactions_are_not_resent_on_the_same_block() {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        true,
        L1BatchCommitmentMode::Rollup,
    )
    .await;

    let _genesis_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let first_l1_batch = TestL1Batch::sealed(&mut tester).await;

    first_l1_batch.save_commit_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;
    // first iteration sends commit tx for the first time
    tester.assert_just_sent_tx_count_equals(1).await;

    tester.run_eth_sender_tx_manager_iteration().await;
    // second iteration re-sends commit tx for the first time
    tester
        .run_eth_sender_tx_manager_iteration_after_n_blocks(0)
        .await;
    // third iteration shouldn't resend the transaction as we're in the same block
    tester.assert_just_sent_tx_count_equals(0).await;
}

#[should_panic(
    expected = "eth-sender was switched to gateway, but there are still 1 pre-gateway transactions in-flight!"
)]
#[test_log::test(tokio::test)]
async fn switching_to_gateway_while_some_transactions_were_in_flight_should_cause_panic() {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        true,
        L1BatchCommitmentMode::Rollup,
    )
    .await;

    let _genesis_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let first_l1_batch = TestL1Batch::sealed(&mut tester).await;

    first_l1_batch.save_commit_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;

    // sanity check
    tester.assert_inflight_txs_count_equals(1).await;

    tester.switch_to_using_gateway();
    tester.run_eth_sender_tx_manager_iteration().await;
}

#[test_log::test(tokio::test)]
async fn switching_to_gateway_works_for_most_basic_scenario() {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        true,
        L1BatchCommitmentMode::Rollup,
    )
    .await;

    let _genesis_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let first_l1_batch = TestL1Batch::sealed(&mut tester).await;

    first_l1_batch.save_commit_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;

    first_l1_batch.execute_commit_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;
    // sanity check
    tester.assert_inflight_txs_count_equals(0).await;

    tester.switch_to_using_gateway();
    tester.run_eth_sender_tx_manager_iteration().await;

    first_l1_batch.save_prove_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;
    tester.assert_inflight_txs_count_equals(1).await;

    first_l1_batch.execute_prove_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;
    tester.assert_inflight_txs_count_equals(0).await;
}

#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn correct_order_for_confirmations(
    commitment_mode: L1BatchCommitmentMode,
) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        true,
        commitment_mode,
    )
    .await;

    let _genesis_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let first_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let second_l1_batch = TestL1Batch::sealed(&mut tester).await;

    first_l1_batch.commit(&mut tester, true).await;
    first_l1_batch.prove(&mut tester, true).await;
    first_l1_batch.execute(&mut tester, true).await;

    second_l1_batch.commit(&mut tester, true).await;
    second_l1_batch.prove(&mut tester, true).await;

    let l1_batches = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_l1_batches(45, None)
        .await
        .unwrap();
    assert_eq!(l1_batches.len(), 1);
    assert_eq!(l1_batches[0].header.number.0, 2);

    second_l1_batch.execute(&mut tester, true).await;
    let l1_batches = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_l1_batches(45, None)
        .await
        .unwrap();
    assert_eq!(l1_batches.len(), 0);
    Ok(())
}

#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn skipped_l1_batch_at_the_start(
    commitment_mode: L1BatchCommitmentMode,
) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        true,
        commitment_mode,
    )
    .await;

    let _genesis_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let first_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let second_l1_batch = TestL1Batch::sealed(&mut tester).await;

    first_l1_batch.commit(&mut tester, true).await;
    first_l1_batch.prove(&mut tester, true).await;
    first_l1_batch.execute(&mut tester, true).await;

    second_l1_batch.commit(&mut tester, true).await;
    second_l1_batch.prove(&mut tester, true).await;
    second_l1_batch.execute(&mut tester, true).await;

    let third_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let fourth_l1_batch = TestL1Batch::sealed(&mut tester).await;

    // DO NOT CONFIRM PROVE TXS
    third_l1_batch.commit(&mut tester, true).await;
    fourth_l1_batch.commit(&mut tester, true).await;
    third_l1_batch.prove(&mut tester, false).await;
    fourth_l1_batch.prove(&mut tester, false).await;

    //sanity check, 2 commit txs are still in-flight
    tester.assert_inflight_txs_count_equals(2).await;

    let l1_batches = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_l1_batches(45, Some(unix_timestamp_ms()))
        .await
        .unwrap();
    assert_eq!(l1_batches.len(), 2);

    third_l1_batch.execute_prove_tx(&mut tester).await;
    let l1_batches = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_l1_batches(45, Some(unix_timestamp_ms()))
        .await
        .unwrap();
    assert_eq!(l1_batches.len(), 2);
    Ok(())
}

#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn skipped_l1_batch_in_the_middle(
    commitment_mode: L1BatchCommitmentMode,
) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        true,
        commitment_mode,
    )
    .await;

    let _genesis_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let first_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let second_l1_batch = TestL1Batch::sealed(&mut tester).await;

    first_l1_batch.commit(&mut tester, true).await;
    first_l1_batch.prove(&mut tester, true).await;
    first_l1_batch.execute(&mut tester, true).await;

    second_l1_batch.commit(&mut tester, true).await;
    second_l1_batch.prove(&mut tester, true).await;

    let third_l1_batch = TestL1Batch::sealed(&mut tester).await;
    let fourth_l1_batch = TestL1Batch::sealed(&mut tester).await;

    // DO NOT CONFIRM THIRD BLOCK
    third_l1_batch.commit(&mut tester, true).await;
    third_l1_batch.prove(&mut tester, false).await;
    fourth_l1_batch.commit(&mut tester, true).await;
    fourth_l1_batch.prove(&mut tester, true).await;

    let l1_batches = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_l1_batches(45, None)
        .await
        .unwrap();
    // We should return all L1 batches including the third one
    assert_eq!(l1_batches.len(), 3);
    assert_eq!(l1_batches[0].header.number.0, 2);

    third_l1_batch.execute_commit_tx(&mut tester).await;
    let l1_batches = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_l1_batches(45, None)
        .await
        .unwrap();
    assert_eq!(l1_batches.len(), 3);
    Ok(())
}

#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn test_parse_multicall_data(commitment_mode: L1BatchCommitmentMode) {
    let tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        false,
        true,
        commitment_mode,
    )
    .await;

    assert!(tester
        .aggregator
        .parse_multicall_data(mock_multicall_response())
        .is_ok());

    let original_wrong_form_data = vec![
        // should contain 5 tuples
        Token::Array(vec![]),
        Token::Array(vec![
            Token::Tuple(vec![]),
            Token::Tuple(vec![]),
            Token::Tuple(vec![]),
        ]),
        Token::Array(vec![Token::Tuple(vec![
            Token::Bool(true),
            Token::Bytes(vec![
                30, 72, 156, 45, 219, 103, 54, 150, 36, 37, 58, 97, 81, 255, 186, 33, 35, 20, 195,
                77, 19, 182, 23, 65, 145, 9, 223, 123, 242, 64, 125, 149,
            ]),
        ])]),
        // should contain 2 tokens in the tuple
        Token::Array(vec![
            Token::Tuple(vec![
                Token::Bool(true),
                Token::Bytes(vec![
                    30, 72, 156, 45, 219, 103, 54, 150, 36, 37, 58, 97, 81, 255, 186, 33, 35, 20,
                    195, 77, 19, 182, 23, 65, 145, 9, 223, 123, 242, 64, 125, 149,
                ]),
                Token::Bytes(vec![]),
            ]),
            Token::Tuple(vec![
                Token::Bool(true),
                Token::Bytes(vec![
                    40, 72, 156, 45, 219, 103, 54, 150, 36, 37, 58, 97, 81, 255, 186, 33, 35, 20,
                    195, 77, 19, 182, 23, 65, 145, 9, 223, 123, 242, 64, 225, 149,
                ]),
            ]),
            Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![3u8; 96])]),
            Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![4u8; 20])]),
            Token::Tuple(vec![
                Token::Bool(true),
                Token::Bytes(
                    H256::from_low_u64_be(ProtocolVersionId::default() as u64)
                        .0
                        .to_vec(),
                ),
            ]),
        ]),
    ];

    for wrong_data_instance in original_wrong_form_data {
        assert_matches!(
            tester
                .aggregator
                .parse_multicall_data(wrong_data_instance.clone()),
            Err(EthSenderError::Parse(Error::InvalidOutputType(_)))
        );
    }
}

#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn get_multicall_data(commitment_mode: L1BatchCommitmentMode) {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        false,
        true,
        commitment_mode,
    )
    .await;
    let multicall_data = tester.aggregator.get_multicall_data().await;
    assert!(multicall_data.is_ok());
}
