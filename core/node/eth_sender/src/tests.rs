use std::time::Duration;

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use zksync_contracts::hyperchain_contract;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::{
    clients::{DynClient, SigningClient, L2},
    BoundEthInterface,
};
use zksync_eth_signer::PrivateKeySigner;
use zksync_l1_contract_interface::{
    i_executor::methods::ExecuteBatches, multicall3::Multicall3Call, Tokenizable,
};
use zksync_node_test_utils::create_l1_batch;
use zksync_types::{
    aggregated_operations::L1BatchAggregatedActionType,
    api::TransactionRequest,
    block::L1BatchHeader,
    commitment::{
        L1BatchCommitmentMode, L1BatchMetaParameters, L1BatchMetadata, L1BatchWithMetadata,
        L2DACommitmentScheme,
    },
    eth_sender::EthTxFinalityStatus,
    ethabi::{self, Token},
    helpers::unix_timestamp_ms,
    settlement::SettlementLayer,
    web3::{self, contract::Error},
    Address, K256PrivateKey, L1BatchNumber, L2ChainId, ProtocolVersionId, SLChainId, H256, U256,
};
use zksync_web3_decl::client::MockClient;

use crate::{
    abstract_l1_interface::{AbstractL1Interface, OperatorType, RealL1Interface},
    aggregated_operations::{AggregatedOperation, L1BatchAggregatedOperation},
    tester::{
        EthSenderTester, TestL1Batch, STATE_TRANSITION_CONTRACT_ADDRESS,
        STATE_TRANSITION_MANAGER_CONTRACT_ADDRESS,
    },
    zksync_functions::ZkSyncFunctions,
    EthSenderError,
};

fn get_dummy_operation(number: u32) -> AggregatedOperation {
    AggregatedOperation::L1Batch(L1BatchAggregatedOperation::Execute(ExecuteBatches {
        l1_batches: vec![L1BatchWithMetadata {
            header: create_l1_batch(number),
            metadata: default_l1_batch_metadata(),
            raw_published_factory_deps: Vec::new(),
        }],
        priority_ops_proofs: Vec::new(),
        dependency_roots: vec![vec![], vec![]],
        logs: vec![vec![], vec![]],
        messages: vec![vec![vec![], vec![]]],
        message_roots: vec![],
    }))
}

const COMMITMENT_MODES: [L1BatchCommitmentMode; 2] = [
    L1BatchCommitmentMode::Rollup,
    L1BatchCommitmentMode::Validium,
];

pub(crate) fn mock_multicall_response(
    call: &web3::CallRequest,
    protocol_version_id: ProtocolVersionId,
) -> Token {
    let functions = ZkSyncFunctions::default();
    let evm_emulator_getter_signature = functions
        .get_evm_emulator_bytecode_hash
        .as_ref()
        .map(ethabi::Function::short_signature);
    let bootloader_signature = functions.get_l2_bootloader_bytecode_hash.short_signature();
    let default_aa_signature = functions
        .get_l2_default_account_bytecode_hash
        .short_signature();
    let evm_emulator_getter_signature = evm_emulator_getter_signature.as_ref().map(|sig| &sig[..]);

    let calldata = &call.data.as_ref().expect("no calldata").0;
    assert_eq!(calldata[..4], functions.aggregate3.short_signature());
    let mut tokens = functions
        .aggregate3
        .decode_input(&calldata[4..])
        .expect("invalid multicall");
    assert_eq!(tokens.len(), 1);
    let Token::Array(tokens) = tokens.pop().unwrap() else {
        panic!("Unexpected input: {tokens:?}");
    };

    let validator_timelock_short_selector = functions
        .state_transition_manager_contract
        .function("validatorTimelock")
        .unwrap()
        .short_signature();
    let valdaitor_timelock_post_v29_short_selector = functions
        .state_transition_manager_contract
        .function("validatorTimelockPostV29")
        .unwrap()
        .short_signature();
    let prototol_version_short_selector = functions
        .state_transition_manager_contract
        .function("protocolVersion")
        .unwrap()
        .short_signature();

    let get_da_validator_pair_selector = functions.get_da_validator_pair.short_signature();
    let execution_delay_selector = functions
        .validator_timelock_contract
        .function("executionDelay")
        .unwrap()
        .short_signature();

    let calls = tokens.into_iter().map(Multicall3Call::from_token);
    let response = calls.map(|call| {
        let call = call.unwrap();
        let output = match &call.calldata[..4] {
            selector if selector == bootloader_signature => {
                assert!(call.target == STATE_TRANSITION_CONTRACT_ADDRESS);
                vec![1u8; 32]
            }
            selector if selector == default_aa_signature => {
                assert!(call.target == STATE_TRANSITION_CONTRACT_ADDRESS);
                vec![2u8; 32]
            }
            selector if Some(selector) == evm_emulator_getter_signature => {
                assert!(call.target == STATE_TRANSITION_CONTRACT_ADDRESS);
                vec![3u8; 32]
            }
            selector if selector == functions.get_verifier_params.short_signature() => {
                assert!(call.target == STATE_TRANSITION_CONTRACT_ADDRESS);
                vec![4u8; 96]
            }
            selector if selector == functions.get_verifier.short_signature() => {
                assert!(call.target == STATE_TRANSITION_CONTRACT_ADDRESS);
                vec![5u8; 32]
            }
            selector if selector == functions.get_protocol_version.short_signature() => {
                assert!(call.target == STATE_TRANSITION_CONTRACT_ADDRESS);
                H256::from_low_u64_be(protocol_version_id as u64).0.to_vec()
            }
            selector if selector == validator_timelock_short_selector => {
                assert!(call.target == STATE_TRANSITION_MANAGER_CONTRACT_ADDRESS);
                vec![6u8; 32]
            }
            selector if selector == prototol_version_short_selector => {
                assert!(call.target == STATE_TRANSITION_MANAGER_CONTRACT_ADDRESS);
                H256::from_low_u64_be(protocol_version_id as u64).0.to_vec()
            }
            selector if selector == get_da_validator_pair_selector => {
                assert!(call.target == STATE_TRANSITION_CONTRACT_ADDRESS);
                let non_zero_address = vec![6u8; 32];

                if protocol_version_id.is_pre_medium_interop() {
                    [non_zero_address.clone(), non_zero_address].concat()
                } else {
                    [
                        non_zero_address.clone(),
                        H256::from_low_u64_be(
                            L2DACommitmentScheme::BlobsAndPubdataKeccak256 as u64,
                        )
                        .0
                        .to_vec(),
                    ]
                    .concat()
                }
            }
            selector if selector == execution_delay_selector => {
                // The target is config_timelock_contract_address which is a random address in tests
                // Return a mock execution delay (e.g., 3600 seconds = 1 hour)
                let execution_delay: u32 = 3600;
                let mut result = vec![0u8; 32];
                result[28..32].copy_from_slice(&execution_delay.to_be_bytes());
                result
            }
            selector if selector == valdaitor_timelock_post_v29_short_selector => {
                assert!(call.target == STATE_TRANSITION_MANAGER_CONTRACT_ADDRESS);
                vec![7u8; 32]
            }
            _ => panic!("unexpected call: {call:?}"),
        };
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(output)])
    });
    Token::Array(response.collect())
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
            evm_emulator_code_hash: None,
            protocol_version: Some(ProtocolVersionId::default()),
        },
        aux_data_hash: H256::default(),
        meta_parameters_hash: H256::default(),
        pass_through_data_hash: H256::default(),
        events_queue_commitment: Some(H256::zero()),
        bootloader_initial_content_commitment: Some(H256::zero()),
        state_diffs_compressed: vec![],
        state_diff_hash: Some(H256::default()),
        local_root: Some(H256::default()),
        aggregation_root: Some(H256::default()),
        da_inclusion_data: Some(vec![]),
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
        SettlementLayer::L1(10.into()),
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
        SettlementLayer::L1(10.into()),
    )
    .await;

    // after this, median should be 6
    tester
        .gateway
        .advance_block_number(3, EthTxFinalityStatus::Finalized);
    tester.gas_adjuster.keep_updated().await?;

    TestL1Batch::sealed(&mut tester).await;

    let block = tester.get_block_numbers().await.latest;

    let tx = tester
        .aggregator
        .save_eth_tx(
            &mut tester.conn.connection().await.unwrap(),
            &get_dummy_operation(0),
            Address::random(),
            ProtocolVersionId::latest(),
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
    tester
        .gateway
        .advance_block_number(2, EthTxFinalityStatus::Finalized);
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
        SettlementLayer::L1(10.into()),
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
            L1BatchAggregatedActionType::Commit,
            true,
            EthTxFinalityStatus::Pending,
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
        SettlementLayer::L1(10.into()),
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

#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn fast_finalization(commitment_mode: L1BatchCommitmentMode) -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = EthSenderTester::new(
        connection_pool.clone(),
        vec![100; 100],
        false,
        true,
        commitment_mode,
        SettlementLayer::L1(10.into()),
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

    first_batch.fast_finalize_commit_tx(&mut tester).await;
    second_batch.fast_finalize_commit_tx(&mut tester).await;

    tester.run_eth_sender_tx_manager_iteration().await;
    // check that last 2 transactions are still considered in-flight
    tester.assert_inflight_txs_count_equals(2).await;
    tester.assert_non_finalized_txs_count_equals(2).await;
    tester.revert_blocks(2).await;
    // After revert we send transactions one by one
    tester.run_eth_sender_tx_manager_iteration().await;
    // Automatically we resend first transaction
    tester.assert_just_sent_tx_count_equals(1).await;
    first_batch.execute_commit_tx(&mut tester).await;
    tester.run_eth_sender_tx_manager_iteration().await;
    // Now we should send all remaining transactions, except the one that was already mined
    tester.assert_just_sent_tx_count_equals(3).await;
    second_batch.execute_commit_tx(&mut tester).await;

    tester.run_eth_sender_tx_manager_iteration().await;
    tester.assert_inflight_txs_count_equals(2).await;
    tester.assert_non_finalized_txs_count_equals(0).await;

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
        SettlementLayer::L1(10.into()),
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
        SettlementLayer::L1(10.into()),
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

    tester
        .run_eth_sender_tx_manager_iteration_after_n_blocks(10)
        .await;
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
        SettlementLayer::L1(10.into()),
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

#[test_log::test(tokio::test)]
async fn switching_to_gateway_works_for_most_basic_scenario() {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        true,
        L1BatchCommitmentMode::Rollup,
        SettlementLayer::L1(10.into()),
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
        SettlementLayer::L1(10.into()),
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
        SettlementLayer::L1(10.into()),
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
        SettlementLayer::L1(10.into()),
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

#[test_casing(2, [false, true])]
#[test_log::test(tokio::test)]
async fn parsing_multicall_data(with_evm_emulator: bool) {
    let tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        false,
        true,
        L1BatchCommitmentMode::Rollup,
        SettlementLayer::L1(10.into()),
    )
    .await;

    let mut mock_response = vec![
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![1u8; 32])]),
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![2u8; 32])]),
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![4u8; 96])]),
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![5u8; 32])]),
        Token::Tuple(vec![
            Token::Bool(true),
            Token::Bytes(
                H256::from_low_u64_be(ProtocolVersionId::latest() as u64)
                    .0
                    .to_vec(),
            ),
        ]),
        Token::Tuple(vec![
            Token::Bool(true),
            Token::Bytes(
                H256::from_low_u64_be(ProtocolVersionId::latest() as u64)
                    .0
                    .to_vec(),
            ),
        ]),
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![6u8; 32])]),
        Token::Tuple(vec![
            Token::Bool(true),
            Token::Bytes(
                [
                    vec![7u8; 32],
                    H256::from_low_u64_be(L2DACommitmentScheme::BlobsAndPubdataKeccak256 as u64)
                        .0
                        .to_vec(),
                ]
                .concat(),
            ),
        ]),
        // Execution delay response (3600 seconds = 0xe10, padded to 32 bytes)
        Token::Tuple(vec![
            Token::Bool(true),
            Token::Bytes({
                let execution_delay: u32 = 3600;
                let mut result = vec![0u8; 32];
                result[28..32].copy_from_slice(&execution_delay.to_be_bytes());
                result
            }),
        ]),
        Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![7u8; 32])]),
    ];
    if with_evm_emulator {
        mock_response.insert(
            2,
            Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![3u8; 32])]),
        );
    }
    let mock_response = Token::Array(mock_response);

    let parsed = tester
        .aggregator
        .parse_multicall_data(mock_response, with_evm_emulator)
        .unwrap();
    assert_eq!(
        parsed.base_system_contracts_hashes.bootloader,
        H256::repeat_byte(1)
    );
    assert_eq!(
        parsed.base_system_contracts_hashes.default_aa,
        H256::repeat_byte(2)
    );
    let expected_evm_emulator_hash = with_evm_emulator.then(|| H256::repeat_byte(3));
    assert_eq!(
        parsed.base_system_contracts_hashes.evm_emulator,
        expected_evm_emulator_hash
    );
    assert_eq!(parsed.verifier_address, Address::repeat_byte(5));
    assert_eq!(
        parsed.chain_protocol_version_id,
        ProtocolVersionId::latest()
    );
    assert_eq!(
        parsed.stm_validator_timelock_address,
        Address::repeat_byte(7)
    );
    assert_eq!(parsed.stm_protocol_version_id, ProtocolVersionId::latest());
    assert_eq!(parsed.execution_delay, Duration::from_secs(3600));
}

#[test_log::test(tokio::test)]
async fn parsing_multicall_data_errors() {
    let tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        false,
        true,
        L1BatchCommitmentMode::Rollup,
        SettlementLayer::L1(10.into()),
    )
    .await;

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
                .parse_multicall_data(wrong_data_instance.clone(), true),
            Err(EthSenderError::Parse(Error::InvalidOutputType(_)))
        );
    }
}

#[test_casing(2, COMMITMENT_MODES)]
#[test_log::test(tokio::test)]
async fn get_multicall_data(commitment_mode: L1BatchCommitmentMode) {
    let mut tester = EthSenderTester::new_with_protocol_version(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        false,
        true,
        commitment_mode,
        SettlementLayer::L1(10.into()),
        ProtocolVersionId::Version28,
    )
    .await;

    let data = tester.aggregator.get_multicall_data().await.unwrap();
    assert_eq!(
        data.base_system_contracts_hashes.bootloader,
        H256::repeat_byte(1)
    );
    assert_eq!(
        data.base_system_contracts_hashes.default_aa,
        H256::repeat_byte(2)
    );
    assert_eq!(data.base_system_contracts_hashes.evm_emulator, None);
    assert_eq!(data.verifier_address, Address::repeat_byte(5));
    assert_eq!(data.chain_protocol_version_id, ProtocolVersionId::Version28);
    assert!(data.da_validator_pair.l2_validator.is_some());

    let commitment_mode = L1BatchCommitmentMode::Rollup;
    let mut tester = EthSenderTester::new_with_protocol_version(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        false,
        true,
        commitment_mode,
        SettlementLayer::L1(10.into()),
        ProtocolVersionId::default(),
    )
    .await;

    let data = tester.aggregator.get_multicall_data().await.unwrap();
    assert_eq!(
        data.base_system_contracts_hashes.bootloader,
        H256::repeat_byte(1)
    );
    assert_eq!(
        data.base_system_contracts_hashes.default_aa,
        H256::repeat_byte(2)
    );
    assert_eq!(data.base_system_contracts_hashes.evm_emulator, None);
    assert_eq!(data.verifier_address, Address::repeat_byte(5));
    assert_eq!(
        data.da_validator_pair.l2_da_commitment_scheme.unwrap(),
        L2DACommitmentScheme::BlobsAndPubdataKeccak256
    );
    assert_eq!(data.chain_protocol_version_id, ProtocolVersionId::latest());
}

#[test_log::test(tokio::test)]
// Tests the encoding of the `EIP712` transaction to
// network format defined by the `EIP`. That is, a signed transaction
// itself and the sidecar containing the blobs.
async fn test_signing_eip712_tx() {
    let chain_id = 10;
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        false,
        false,
        L1BatchCommitmentMode::Rollup,
        SettlementLayer::Gateway(chain_id.into()),
    )
    .await;

    let private_key = "27593fea79697e947890ecbecce7901b0008345e5d7259710d0dd5e500d040be"
        .parse()
        .unwrap();
    let private_key = K256PrivateKey::from_bytes(private_key).unwrap();

    let signer = PrivateKeySigner::new(private_key);
    let client = MockClient::builder(L2ChainId::new(chain_id).unwrap().into()).build();
    let client = Box::new(client) as Box<DynClient<L2>>;
    let sign_client = Box::new(SigningClient::new(
        client,
        hyperchain_contract(),
        Address::random(),
        signer,
        Address::zero(),
        U256::one(),
        SLChainId(chain_id),
    )) as Box<dyn BoundEthInterface>;
    let l1_interface = RealL1Interface {
        ethereum_client: None,
        ethereum_client_blobs: None,
        sl_client: Some(sign_client),
        wait_confirmations: Some(10),
    };

    tester.seal_l1_batch().await;
    let header = tester.seal_l1_batch().await;
    let eth_tx = tester.save_commit_tx(header.number).await;

    let tx = l1_interface
        .sign_tx(
            &eth_tx,
            0,
            0,
            None,
            Default::default(),
            OperatorType::Gateway,
            Some(1.into()),
        )
        .await;
    let (_tx_req, _tx_hash) =
        TransactionRequest::from_bytes(tx.raw_tx.as_ref(), L2ChainId::new(chain_id).unwrap())
            .unwrap();
}

#[test_log::test(tokio::test)]
async fn manager_monitors_even_unsuccesfully_sent_txs() {
    let pool = ConnectionPool::<Core>::test_pool().await;

    let mut tester = EthSenderTester::new(
        pool.clone(),
        vec![100; 100],
        false,
        true,
        L1BatchCommitmentMode::Rollup,
        SettlementLayer::L1(10.into()),
    )
    .await;

    let _genesis_batch = TestL1Batch::sealed(&mut tester).await;
    let l1_batch = TestL1Batch::sealed(&mut tester).await;
    l1_batch.save_commit_tx(&mut tester).await;
    tester.gateway_blobs.set_return_error_on_tx_request(true);

    tester.run_eth_sender_tx_manager_iteration().await;

    // check that we sent something and stored it in the db.
    tester.assert_just_sent_tx_count_equals(1).await;
    // tx should be considered in-flight.
    tester.assert_inflight_txs_count_equals(1).await;

    let mut conn = pool.connection().await.unwrap();
    let tx = conn
        .eth_sender_dal()
        .get_last_sent_successfully_eth_tx_by_batch_and_op(
            L1BatchNumber(1),
            L1BatchAggregatedActionType::Commit,
        )
        .await;
    assert!(tx.is_none());

    let all_attempts = conn
        .eth_sender_dal()
        .get_tx_history_to_check(1)
        .await
        .unwrap();
    assert_eq!(all_attempts.len(), 1);

    // Mark tx as successful on SL side and run eth tx manager iteration.
    tester.confirm_tx(all_attempts[0].tx_hash, true).await;

    // Check that `sent_successfully` was reset to true.
    let tx = conn
        .eth_sender_dal()
        .get_last_sent_successfully_eth_tx_by_batch_and_op(
            L1BatchNumber(1),
            L1BatchAggregatedActionType::Commit,
        )
        .await
        .unwrap();
    assert!(tx.sent_successfully);

    // Check that tx is confirmed.
    let is_confirmed = conn
        .eth_sender_dal()
        .get_confirmed_tx_hash_by_eth_tx_id(tx.eth_tx_id)
        .await
        .unwrap()
        .is_some();
    assert!(is_confirmed);
}
