use std::sync::Arc;

use assert_matches::assert_matches;
use once_cell::sync::Lazy;
use test_casing::{test_casing, Product};
use zksync_config::{
    configs::eth_sender::{ProofSendingMode, PubdataSendingMode, SenderConfig},
    ContractsConfig, EthConfig, GasAdjusterConfig,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::clients::MockEthereum;
use zksync_l1_contract_interface::i_executor::methods::{ExecuteBatches, ProveBatches};
use zksync_mini_merkle_tree::SyncMerkleTree;
use zksync_node_fee_model::l1_gas_price::GasAdjuster;
use zksync_node_test_utils::{create_l1_batch, l1_batch_metadata_to_commitment_artifacts};
use zksync_object_store::MockObjectStore;
use zksync_types::{
    block::L1BatchHeader,
    commitment::{
        L1BatchCommitmentMode, L1BatchMetaParameters, L1BatchMetadata, L1BatchWithMetadata,
    },
    ethabi::Token,
    helpers::unix_timestamp_ms,
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    pubdata_da::PubdataDA,
    web3::contract::Error,
    Address, L1BatchNumber, ProtocolVersion, ProtocolVersionId, H256,
};

use crate::{
    abstract_l1_interface::L1BlockNumbers, aggregated_operations::AggregatedOperation, Aggregator,
    EthSenderError, EthTxAggregator, EthTxManager,
};

// Alias to conveniently call static methods of `ETHSender`.
type MockEthTxManager = EthTxManager;

static DUMMY_OPERATION: Lazy<AggregatedOperation> = Lazy::new(|| {
    AggregatedOperation::Execute(ExecuteBatches {
        l1_batches: vec![L1BatchWithMetadata {
            header: create_l1_batch(1),
            metadata: default_l1_batch_metadata(),
            raw_published_factory_deps: Vec::new(),
        }],
        priority_ops_proofs: Vec::new(),
    })
});

fn get_dummy_operation(number: u32) -> AggregatedOperation {
    AggregatedOperation::Execute(ExecuteBatches {
        l1_batches: vec![L1BatchWithMetadata {
            header: create_l1_batch(number),
            metadata: default_l1_batch_metadata(),
            raw_published_factory_deps: Vec::new(),
        }],
        priority_ops_proofs: Vec::new(),
    })
}

const COMMITMENT_MODES: [L1BatchCommitmentMode; 2] = [
    L1BatchCommitmentMode::Rollup,
    L1BatchCommitmentMode::Validium,
];

fn mock_l1_batch_header(number: u32) -> L1BatchHeader {
    let mut header = L1BatchHeader::new(
        L1BatchNumber(number),
        100,
        BaseSystemContractsHashes {
            bootloader: H256::repeat_byte(1),
            default_aa: H256::repeat_byte(42),
        },
        ProtocolVersionId::latest(),
    );
    header.l1_tx_count = 3;
    header.l2_tx_count = 5;
    header.l2_to_l1_logs.push(UserL2ToL1Log(L2ToL1Log {
        shard_id: 0,
        is_service: false,
        tx_number_in_block: 2,
        sender: Address::repeat_byte(2),
        key: H256::repeat_byte(3),
        value: H256::zero(),
    }));
    header.l2_to_l1_messages.push(vec![22; 22]);
    header.l2_to_l1_messages.push(vec![33; 33]);

    header
}

fn mock_multicall_response() -> Token {
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

#[derive(Debug)]
struct EthSenderTester {
    conn: ConnectionPool<Core>,
    gateway: Box<MockEthereum>,
    manager: MockEthTxManager,
    aggregator: EthTxAggregator,
    gas_adjuster: Arc<GasAdjuster>,
}

impl EthSenderTester {
    const WAIT_CONFIRMATIONS: u64 = 10;
    const MAX_BASE_FEE_SAMPLES: usize = 3;

    async fn new(
        connection_pool: ConnectionPool<Core>,
        history: Vec<u64>,
        non_ordering_confirmations: bool,
        aggregator_operate_4844_mode: bool,
        commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        let eth_sender_config = EthConfig::for_tests();
        let contracts_config = ContractsConfig::for_tests();
        let aggregator_config = SenderConfig {
            aggregated_proof_sizes: vec![1],
            ..eth_sender_config.clone().sender.unwrap()
        };

        let gateway = MockEthereum::builder()
            .with_fee_history(
                std::iter::repeat(0)
                    .take(Self::WAIT_CONFIRMATIONS as usize)
                    .chain(history)
                    .collect(),
            )
            .with_non_ordering_confirmation(non_ordering_confirmations)
            .with_call_handler(move |call, _| {
                assert_eq!(call.to, Some(contracts_config.l1_multicall3_addr));
                mock_multicall_response()
            })
            .build();
        gateway.advance_block_number(Self::WAIT_CONFIRMATIONS);
        let gateway = Box::new(gateway);

        let gas_adjuster = Arc::new(
            GasAdjuster::new(
                Box::new(gateway.clone().into_client()),
                GasAdjusterConfig {
                    max_base_fee_samples: Self::MAX_BASE_FEE_SAMPLES,
                    pricing_formula_parameter_a: 3.0,
                    pricing_formula_parameter_b: 2.0,
                    ..eth_sender_config.gas_adjuster.unwrap()
                },
                PubdataSendingMode::Calldata,
                commitment_mode,
            )
            .await
            .unwrap(),
        );

        let eth_sender = eth_sender_config.sender.clone().unwrap();
        let aggregator = EthTxAggregator::new(
            connection_pool.clone(),
            SenderConfig {
                proof_sending_mode: ProofSendingMode::SkipEveryProof,
                pubdata_sending_mode: PubdataSendingMode::Calldata,
                ..eth_sender.clone()
            },
            // Aggregator - unused
            Aggregator::new(
                aggregator_config.clone(),
                MockObjectStore::arc(),
                aggregator_operate_4844_mode,
                commitment_mode,
                SyncMerkleTree::from_hashes(std::iter::empty(), None),
            ),
            gateway.clone(),
            // zkSync contract address
            Address::random(),
            contracts_config.l1_multicall3_addr,
            Address::random(),
            Default::default(),
            None,
        )
        .await;

        let manager = EthTxManager::new(
            connection_pool.clone(),
            eth_sender.clone(),
            gas_adjuster.clone(),
            gateway.clone(),
            None,
        );
        Self {
            gateway,
            manager,
            aggregator,
            gas_adjuster,
            conn: connection_pool,
        }
    }

    async fn storage(&self) -> Connection<'_, Core> {
        self.conn.connection().await.unwrap()
    }

    async fn get_block_numbers(&self) -> L1BlockNumbers {
        let latest = self
            .manager
            .l1_interface()
            .get_l1_block_numbers()
            .await
            .unwrap()
            .latest;
        let finalized = latest - Self::WAIT_CONFIRMATIONS as u32;
        L1BlockNumbers {
            finalized,
            latest,
            safe: finalized,
        }
    }
}

fn l1_batch_with_metadata(header: L1BatchHeader) -> L1BatchWithMetadata {
    L1BatchWithMetadata {
        header,
        metadata: default_l1_batch_metadata(),
        raw_published_factory_deps: vec![],
    }
}

fn default_l1_batch_metadata() -> L1BatchMetadata {
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
        state_diff_hash: H256::default(),
    }
}

// Tests that we send multiple transactions and confirm them all in one iteration.
#[test_casing(4, Product(([false, true], COMMITMENT_MODES)))]
#[tokio::test]
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

    let mut hashes = vec![];

    connection_pool
        .clone()
        .connection()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    for number in 0..5 {
        connection_pool
            .clone()
            .connection()
            .await
            .unwrap()
            .blocks_dal()
            .insert_mock_l1_batch(&mock_l1_batch_header(number + 1))
            .await
            .unwrap();
        let tx = tester
            .aggregator
            .save_eth_tx(
                &mut tester.conn.connection().await.unwrap(),
                &get_dummy_operation(number + 1),
                false,
            )
            .await?;
        let hash = tester
            .manager
            .send_eth_tx(
                &mut tester.conn.connection().await.unwrap(),
                &tx,
                0,
                tester.get_block_numbers().await.latest,
            )
            .await?;
        hashes.push(hash);
    }

    // check that we sent something
    assert_eq!(tester.gateway.sent_tx_count(), 5);
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .unwrap()
            .len(),
        5
    );

    for hash in hashes {
        tester
            .gateway
            .execute_tx(hash, true, EthSenderTester::WAIT_CONFIRMATIONS);
    }

    let to_resend = tester
        .manager
        .monitor_inflight_transactions(
            &mut tester.conn.connection().await.unwrap(),
            tester.get_block_numbers().await,
        )
        .await?;

    // check that transaction is marked as accepted
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .unwrap()
            .len(),
        0
    );

    // also check that we didn't try to resend it
    assert!(to_resend.is_none());

    Ok(())
}

// Tests that we resend first un-mined transaction every block with an increased gas price.
#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn resend_each_block(commitment_mode: L1BatchCommitmentMode) -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = EthSenderTester::new(
        connection_pool.clone(),
        vec![7, 6, 5, 5, 5, 2, 1],
        false,
        false,
        commitment_mode,
    )
    .await;

    // after this, median should be 6
    tester.gateway.advance_block_number(3);
    tester.gas_adjuster.keep_updated().await?;

    connection_pool
        .clone()
        .connection()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    connection_pool
        .clone()
        .connection()
        .await
        .unwrap()
        .blocks_dal()
        .insert_mock_l1_batch(&mock_l1_batch_header(1))
        .await
        .unwrap();

    let block = tester.get_block_numbers().await.latest;

    let tx = tester
        .aggregator
        .save_eth_tx(
            &mut tester.conn.connection().await.unwrap(),
            &get_dummy_operation(1),
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
            .get_inflight_txs()
            .await
            .unwrap()
            .len(),
        1
    );

    let sent_tx = tester
        .manager
        .l1_interface()
        .get_tx(hash)
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
        .monitor_inflight_transactions(&mut tester.conn.connection().await.unwrap(), block_numbers)
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
            .get_inflight_txs()
            .await
            .unwrap()
            .len(),
        1
    );

    let resent_tx = tester
        .manager
        .l1_interface()
        .get_tx(resent_hash)
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
#[tokio::test]
async fn dont_resend_already_mined(commitment_mode: L1BatchCommitmentMode) -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = EthSenderTester::new(
        connection_pool.clone(),
        vec![100; 100],
        false,
        false,
        commitment_mode,
    )
    .await;

    connection_pool
        .clone()
        .connection()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    connection_pool
        .clone()
        .connection()
        .await
        .unwrap()
        .blocks_dal()
        .insert_mock_l1_batch(&mock_l1_batch_header(1))
        .await
        .unwrap();

    let tx = tester
        .aggregator
        .save_eth_tx(
            &mut tester.conn.connection().await.unwrap(),
            &DUMMY_OPERATION,
            false,
        )
        .await
        .unwrap();

    let hash = tester
        .manager
        .send_eth_tx(
            &mut tester.conn.connection().await.unwrap(),
            &tx,
            0,
            tester.get_block_numbers().await.latest,
        )
        .await
        .unwrap();

    // check that we sent something and stored it in the db
    assert_eq!(tester.gateway.sent_tx_count(), 1);
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .unwrap()
            .len(),
        1
    );

    // mine the transaction but don't have enough confirmations yet
    tester
        .gateway
        .execute_tx(hash, true, EthSenderTester::WAIT_CONFIRMATIONS - 1);

    let to_resend = tester
        .manager
        .monitor_inflight_transactions(
            &mut tester.conn.connection().await.unwrap(),
            tester.get_block_numbers().await,
        )
        .await?;

    // check that transaction is still considered in-flight
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .unwrap()
            .len(),
        1
    );

    // also check that we didn't try to resend it
    assert!(to_resend.is_none());

    Ok(())
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn three_scenarios(commitment_mode: L1BatchCommitmentMode) -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = EthSenderTester::new(
        connection_pool.clone(),
        vec![100; 100],
        false,
        false,
        commitment_mode,
    )
    .await;

    let mut hashes = vec![];

    connection_pool
        .clone()
        .connection()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    for number in 0..3 {
        connection_pool
            .clone()
            .connection()
            .await
            .unwrap()
            .blocks_dal()
            .insert_mock_l1_batch(&mock_l1_batch_header(number + 1))
            .await
            .unwrap();
        let tx = tester
            .aggregator
            .save_eth_tx(
                &mut tester.conn.connection().await.unwrap(),
                &get_dummy_operation(number + 1),
                false,
            )
            .await
            .unwrap();

        let hash = tester
            .manager
            .send_eth_tx(
                &mut tester.conn.connection().await.unwrap(),
                &tx,
                0,
                tester.get_block_numbers().await.latest,
            )
            .await
            .unwrap();

        hashes.push(hash);
    }

    // check that we sent something
    assert_eq!(tester.gateway.sent_tx_count(), 3);

    // mined & confirmed
    tester
        .gateway
        .execute_tx(hashes[0], true, EthSenderTester::WAIT_CONFIRMATIONS);
    // mined but not confirmed
    tester
        .gateway
        .execute_tx(hashes[1], true, EthSenderTester::WAIT_CONFIRMATIONS - 1);

    let (to_resend, _) = tester
        .manager
        .monitor_inflight_transactions(
            &mut tester.conn.connection().await.unwrap(),
            tester.get_block_numbers().await,
        )
        .await?
        .expect("we should be trying to resend the last tx");

    // check that last 2 transactions are still considered in-flight
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .unwrap()
            .len(),
        2
    );

    // last sent transaction has nonce == 2, because they start from 0
    assert_eq!(to_resend.nonce.0, 2);

    Ok(())
}

#[should_panic(expected = "We can't operate after tx fail")]
#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn failed_eth_tx(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = EthSenderTester::new(
        connection_pool.clone(),
        vec![100; 100],
        false,
        false,
        commitment_mode,
    )
    .await;

    connection_pool
        .clone()
        .connection()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    connection_pool
        .clone()
        .connection()
        .await
        .unwrap()
        .blocks_dal()
        .insert_mock_l1_batch(&mock_l1_batch_header(1))
        .await
        .unwrap();

    let tx = tester
        .aggregator
        .save_eth_tx(
            &mut tester.conn.connection().await.unwrap(),
            &DUMMY_OPERATION,
            false,
        )
        .await
        .unwrap();

    let hash = tester
        .manager
        .send_eth_tx(
            &mut tester.conn.connection().await.unwrap(),
            &tx,
            0,
            tester.get_block_numbers().await.latest,
        )
        .await
        .unwrap();

    // fail this tx
    tester
        .gateway
        .execute_tx(hash, false, EthSenderTester::WAIT_CONFIRMATIONS);
    tester
        .manager
        .monitor_inflight_transactions(
            &mut tester.conn.connection().await.unwrap(),
            tester.get_block_numbers().await,
        )
        .await
        .unwrap();
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn correct_order_for_confirmations(
    commitment_mode: L1BatchCommitmentMode,
) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        false,
        commitment_mode,
    )
    .await;

    insert_genesis_protocol_version(&tester).await;
    let genesis_l1_batch = insert_l1_batch(&tester, L1BatchNumber(0)).await;
    let first_l1_batch = insert_l1_batch(&tester, L1BatchNumber(1)).await;
    let second_l1_batch = insert_l1_batch(&tester, L1BatchNumber(2)).await;

    commit_l1_batch(
        &mut tester,
        genesis_l1_batch.clone(),
        first_l1_batch.clone(),
        true,
    )
    .await;
    prove_l1_batch(
        &mut tester,
        genesis_l1_batch.clone(),
        first_l1_batch.clone(),
        true,
    )
    .await;
    execute_l1_batches(&mut tester, vec![first_l1_batch.clone()], true).await;
    commit_l1_batch(
        &mut tester,
        first_l1_batch.clone(),
        second_l1_batch.clone(),
        true,
    )
    .await;
    prove_l1_batch(
        &mut tester,
        first_l1_batch.clone(),
        second_l1_batch.clone(),
        true,
    )
    .await;

    let l1_batches = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_l1_batches(45, None)
        .await
        .unwrap();
    assert_eq!(l1_batches.len(), 1);
    assert_eq!(l1_batches[0].header.number.0, 2);

    execute_l1_batches(&mut tester, vec![second_l1_batch.clone()], true).await;
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
#[tokio::test]
async fn skipped_l1_batch_at_the_start(
    commitment_mode: L1BatchCommitmentMode,
) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        false,
        commitment_mode,
    )
    .await;

    insert_genesis_protocol_version(&tester).await;
    let genesis_l1_batch = insert_l1_batch(&tester, L1BatchNumber(0)).await;
    let first_l1_batch = insert_l1_batch(&tester, L1BatchNumber(1)).await;
    let second_l1_batch = insert_l1_batch(&tester, L1BatchNumber(2)).await;

    commit_l1_batch(
        &mut tester,
        genesis_l1_batch.clone(),
        first_l1_batch.clone(),
        true,
    )
    .await;
    prove_l1_batch(
        &mut tester,
        genesis_l1_batch.clone(),
        first_l1_batch.clone(),
        true,
    )
    .await;
    execute_l1_batches(&mut tester, vec![first_l1_batch.clone()], true).await;
    commit_l1_batch(
        &mut tester,
        first_l1_batch.clone(),
        second_l1_batch.clone(),
        true,
    )
    .await;
    prove_l1_batch(
        &mut tester,
        first_l1_batch.clone(),
        second_l1_batch.clone(),
        true,
    )
    .await;
    execute_l1_batches(&mut tester, vec![second_l1_batch.clone()], true).await;

    let third_l1_batch = insert_l1_batch(&tester, L1BatchNumber(3)).await;
    let fourth_l1_batch = insert_l1_batch(&tester, L1BatchNumber(4)).await;
    // DO NOT CONFIRM THIRD BLOCK
    let third_l1_batch_commit_tx_hash = commit_l1_batch(
        &mut tester,
        second_l1_batch.clone(),
        third_l1_batch.clone(),
        false,
    )
    .await;

    prove_l1_batch(
        &mut tester,
        second_l1_batch.clone(),
        third_l1_batch.clone(),
        true,
    )
    .await;
    commit_l1_batch(
        &mut tester,
        third_l1_batch.clone(),
        fourth_l1_batch.clone(),
        true,
    )
    .await;
    prove_l1_batch(
        &mut tester,
        third_l1_batch.clone(),
        fourth_l1_batch.clone(),
        true,
    )
    .await;
    let l1_batches = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_l1_batches(45, Some(unix_timestamp_ms()))
        .await
        .unwrap();
    assert_eq!(l1_batches.len(), 2);

    confirm_tx(&mut tester, third_l1_batch_commit_tx_hash).await;
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
#[tokio::test]
async fn skipped_l1_batch_in_the_middle(
    commitment_mode: L1BatchCommitmentMode,
) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        true,
        false,
        commitment_mode,
    )
    .await;

    insert_genesis_protocol_version(&tester).await;
    let genesis_l1_batch = insert_l1_batch(&tester, L1BatchNumber(0)).await;
    let first_l1_batch = insert_l1_batch(&tester, L1BatchNumber(1)).await;
    let second_l1_batch = insert_l1_batch(&tester, L1BatchNumber(2)).await;
    commit_l1_batch(
        &mut tester,
        genesis_l1_batch.clone(),
        first_l1_batch.clone(),
        true,
    )
    .await;
    prove_l1_batch(&mut tester, genesis_l1_batch, first_l1_batch.clone(), true).await;
    execute_l1_batches(&mut tester, vec![first_l1_batch.clone()], true).await;
    commit_l1_batch(
        &mut tester,
        first_l1_batch.clone(),
        second_l1_batch.clone(),
        true,
    )
    .await;
    prove_l1_batch(
        &mut tester,
        first_l1_batch.clone(),
        second_l1_batch.clone(),
        true,
    )
    .await;

    let third_l1_batch = insert_l1_batch(&tester, L1BatchNumber(3)).await;
    let fourth_l1_batch = insert_l1_batch(&tester, L1BatchNumber(4)).await;
    // DO NOT CONFIRM THIRD BLOCK
    let third_l1_batch_commit_tx_hash = commit_l1_batch(
        &mut tester,
        second_l1_batch.clone(),
        third_l1_batch.clone(),
        false,
    )
    .await;

    prove_l1_batch(
        &mut tester,
        second_l1_batch.clone(),
        third_l1_batch.clone(),
        true,
    )
    .await;
    commit_l1_batch(
        &mut tester,
        third_l1_batch.clone(),
        fourth_l1_batch.clone(),
        true,
    )
    .await;
    prove_l1_batch(
        &mut tester,
        third_l1_batch.clone(),
        fourth_l1_batch.clone(),
        true,
    )
    .await;
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

    confirm_tx(&mut tester, third_l1_batch_commit_tx_hash).await;
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
#[tokio::test]
async fn test_parse_multicall_data(commitment_mode: L1BatchCommitmentMode) {
    let tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        false,
        false,
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
#[tokio::test]
async fn get_multicall_data(commitment_mode: L1BatchCommitmentMode) {
    let mut tester = EthSenderTester::new(
        ConnectionPool::<Core>::test_pool().await,
        vec![100; 100],
        false,
        false,
        commitment_mode,
    )
    .await;
    let multicall_data = tester.aggregator.get_multicall_data().await;
    assert!(multicall_data.is_ok());
}

async fn insert_genesis_protocol_version(tester: &EthSenderTester) {
    tester
        .storage()
        .await
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
}

async fn insert_l1_batch(tester: &EthSenderTester, number: L1BatchNumber) -> L1BatchHeader {
    let header = create_l1_batch(number.0);

    // Save L1 batch to the database
    tester
        .storage()
        .await
        .blocks_dal()
        .insert_mock_l1_batch(&header)
        .await
        .unwrap();
    let metadata = default_l1_batch_metadata();
    tester
        .storage()
        .await
        .blocks_dal()
        .save_l1_batch_tree_data(header.number, &metadata.tree_data())
        .await
        .unwrap();
    tester
        .storage()
        .await
        .blocks_dal()
        .save_l1_batch_commitment_artifacts(
            header.number,
            &l1_batch_metadata_to_commitment_artifacts(&metadata),
        )
        .await
        .unwrap();
    header
}

async fn execute_l1_batches(
    tester: &mut EthSenderTester,
    l1_batches: Vec<L1BatchHeader>,
    confirm: bool,
) -> H256 {
    let operation = AggregatedOperation::Execute(ExecuteBatches {
        priority_ops_proofs: l1_batches.iter().map(|_| Default::default()).collect(),
        l1_batches: l1_batches.into_iter().map(l1_batch_with_metadata).collect(),
    });
    send_operation(tester, operation, confirm).await
}

async fn prove_l1_batch(
    tester: &mut EthSenderTester,
    last_committed_l1_batch: L1BatchHeader,
    l1_batch: L1BatchHeader,
    confirm: bool,
) -> H256 {
    let operation = AggregatedOperation::PublishProofOnchain(ProveBatches {
        prev_l1_batch: l1_batch_with_metadata(last_committed_l1_batch),
        l1_batches: vec![l1_batch_with_metadata(l1_batch)],
        proofs: vec![],
        should_verify: false,
    });
    send_operation(tester, operation, confirm).await
}

async fn commit_l1_batch(
    tester: &mut EthSenderTester,
    last_committed_l1_batch: L1BatchHeader,
    l1_batch: L1BatchHeader,
    confirm: bool,
) -> H256 {
    let operation = AggregatedOperation::Commit(
        l1_batch_with_metadata(last_committed_l1_batch),
        vec![l1_batch_with_metadata(l1_batch)],
        PubdataDA::Calldata,
    );
    send_operation(tester, operation, confirm).await
}

async fn send_operation(
    tester: &mut EthSenderTester,
    aggregated_operation: AggregatedOperation,
    confirm: bool,
) -> H256 {
    let tx = tester
        .aggregator
        .save_eth_tx(
            &mut tester.conn.connection().await.unwrap(),
            &aggregated_operation,
            false,
        )
        .await
        .unwrap();

    let hash = tester
        .manager
        .send_eth_tx(
            &mut tester.conn.connection().await.unwrap(),
            &tx,
            0,
            tester.get_block_numbers().await.latest,
        )
        .await
        .unwrap();

    if confirm {
        confirm_tx(tester, hash).await;
    }
    hash
}

async fn confirm_tx(tester: &mut EthSenderTester, hash: H256) {
    tester
        .gateway
        .execute_tx(hash, true, EthSenderTester::WAIT_CONFIRMATIONS);
    tester
        .manager
        .monitor_inflight_transactions(
            &mut tester.conn.connection().await.unwrap(),
            tester.get_block_numbers().await,
        )
        .await
        .unwrap();
}
