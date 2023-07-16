use std::sync::{atomic::Ordering, Arc};

use db_test_macro::db_test;
use zksync_config::{
    configs::eth_sender::{ProofSendingMode, SenderConfig},
    ETHSenderConfig, GasAdjusterConfig,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::{clients::mock::MockEthereum, EthInterface};
use zksync_types::{
    aggregated_operations::{
        AggregatedOperation, BlocksCommitOperation, BlocksExecuteOperation, BlocksProofOperation,
    },
    block::L1BatchHeader,
    commitment::{BlockMetaParameters, BlockMetadata, BlockWithMetadata},
    helpers::unix_timestamp_ms,
    Address, L1BatchNumber, L1BlockNumber, H256,
};

use crate::eth_sender::{
    eth_tx_manager::L1BlockNumbers, Aggregator, EthTxAggregator, EthTxManager,
};
use crate::l1_gas_price::GasAdjuster;

// Alias to conveniently call static methods of ETHSender.
type MockEthTxManager = EthTxManager<Arc<MockEthereum>, GasAdjuster<Arc<MockEthereum>>>;

const DUMMY_OPERATION: AggregatedOperation =
    AggregatedOperation::ExecuteBlocks(BlocksExecuteOperation { blocks: vec![] });

#[derive(Debug)]
struct EthSenderTester {
    conn: ConnectionPool,
    gateway: Arc<MockEthereum>,
    manager: MockEthTxManager,
    aggregator: EthTxAggregator,
    gas_adjuster: Arc<GasAdjuster<Arc<MockEthereum>>>,
}

impl EthSenderTester {
    const WAIT_CONFIRMATIONS: u64 = 10;
    const MAX_BASE_FEE_SAMPLES: usize = 3;

    async fn new(
        connection_pool: ConnectionPool,
        history: Vec<u64>,
        non_ordering_confirmations: bool,
    ) -> Self {
        let eth_sender_config = ETHSenderConfig::from_env();
        let aggregator_config = SenderConfig {
            aggregated_proof_sizes: vec![1],
            ..eth_sender_config.sender.clone()
        };

        let gateway = Arc::new(
            MockEthereum::default()
                .with_fee_history(
                    std::iter::repeat(0)
                        .take(Self::WAIT_CONFIRMATIONS as usize)
                        .chain(history)
                        .collect(),
                )
                .with_non_ordering_confirmation(non_ordering_confirmations),
        );
        gateway
            .block_number
            .fetch_add(Self::WAIT_CONFIRMATIONS, Ordering::Relaxed);

        let gas_adjuster = Arc::new(
            GasAdjuster::new(
                gateway.clone(),
                GasAdjusterConfig {
                    max_base_fee_samples: Self::MAX_BASE_FEE_SAMPLES,
                    pricing_formula_parameter_a: 3.0,
                    pricing_formula_parameter_b: 2.0,
                    ..eth_sender_config.gas_adjuster
                },
            )
            .await
            .unwrap(),
        );

        let aggregator = EthTxAggregator::new(
            SenderConfig {
                proof_sending_mode: ProofSendingMode::SkipEveryProof,
                ..eth_sender_config.sender.clone()
            },
            // Aggregator - unused
            Aggregator::new(aggregator_config.clone()),
            // zkSync contract address
            Address::random(),
            0,
        );

        let manager = EthTxManager::new(
            eth_sender_config.sender,
            gas_adjuster.clone(),
            gateway.clone(),
        );
        Self {
            gateway,
            manager,
            aggregator,
            gas_adjuster,
            conn: connection_pool,
        }
    }

    async fn storage(&self) -> StorageProcessor<'static> {
        self.conn.access_test_storage().await
    }

    async fn get_block_numbers(&self) -> L1BlockNumbers {
        let latest = self.gateway.block_number("").await.unwrap().as_u32().into();
        let finalized = latest - Self::WAIT_CONFIRMATIONS as u32;
        L1BlockNumbers { finalized, latest }
    }
}

// Tests that we send multiple transactions and confirm them all in one iteration.
#[db_test]
async fn confirm_many(connection_pool: ConnectionPool) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(connection_pool, vec![10; 100], false).await;

    let mut hashes = vec![];

    for _ in 0..5 {
        let tx = tester
            .aggregator
            .save_eth_tx(&mut tester.storage().await, &DUMMY_OPERATION)
            .await?;
        let hash = tester
            .manager
            .send_eth_tx(
                &mut tester.storage().await,
                &tx,
                0,
                L1BlockNumber(tester.gateway.block_number("").await?.as_u32()),
            )
            .await?;
        hashes.push(hash);
    }

    // check that we sent something
    assert_eq!(tester.gateway.sent_txs.read().unwrap().len(), 5);
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .len(),
        5
    );

    for hash in hashes {
        tester
            .gateway
            .execute_tx(hash, true, EthSenderTester::WAIT_CONFIRMATIONS)?;
    }

    let to_resend = tester
        .manager
        .monitor_inflight_transactions(
            &mut tester.storage().await,
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
            .len(),
        0
    );

    // also check that we didn't try to resend it
    assert!(to_resend.is_none());

    Ok(())
}

// Tests that we resend first unmined transaction every block with an increased gas price.
#[db_test]
async fn resend_each_block(connection_pool: ConnectionPool) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(connection_pool, vec![7, 6, 5, 5, 5, 2, 1], false).await;

    // after this, median should be 6
    tester.gateway.advance_block_number(3);
    tester.gas_adjuster.keep_updated().await?;

    let block = L1BlockNumber(tester.gateway.block_number("").await?.as_u32());
    let tx = tester
        .aggregator
        .save_eth_tx(&mut tester.storage().await, &DUMMY_OPERATION)
        .await?;

    let hash = tester
        .manager
        .send_eth_tx(&mut tester.storage().await, &tx, 0, block)
        .await?;

    // check that we sent something and stored it in the db
    assert_eq!(tester.gateway.sent_txs.read().unwrap().len(), 1);
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .len(),
        1
    );

    let sent_tx = tester.gateway.sent_txs.read().unwrap()[&hash];
    assert_eq!(sent_tx.hash, hash);
    assert_eq!(sent_tx.nonce, 0);
    assert_eq!(sent_tx.base_fee.as_usize(), 18); // 6 * 3 * 2^0

    // now, median is 5
    tester.gateway.advance_block_number(2);
    tester.gas_adjuster.keep_updated().await?;
    let block_numbers = tester.get_block_numbers().await;

    let (to_resend, _) = tester
        .manager
        .monitor_inflight_transactions(&mut tester.storage().await, block_numbers)
        .await?
        .unwrap();

    let resent_hash = tester
        .manager
        .send_eth_tx(
            &mut tester.storage().await,
            &to_resend,
            1,
            block_numbers.latest,
        )
        .await?;

    // check that transaction has been resent
    assert_eq!(tester.gateway.sent_txs.read().unwrap().len(), 2);
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .len(),
        1
    );

    let resent_tx = tester.gateway.sent_txs.read().unwrap()[&resent_hash];
    assert_eq!(resent_tx.nonce, 0);
    assert_eq!(resent_tx.base_fee.as_usize(), 30); // 5 * 3 * 2^1

    Ok(())
}

// Tests that if transaction was mined, but not enough blocks has been mined since,
// we won't mark it as confirmed but also won't resend it.
#[db_test]
async fn dont_resend_already_mined(connection_pool: ConnectionPool) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(connection_pool, vec![100; 100], false).await;
    let tx = tester
        .aggregator
        .save_eth_tx(&mut tester.storage().await, &DUMMY_OPERATION)
        .await
        .unwrap();

    let hash = tester
        .manager
        .send_eth_tx(
            &mut tester.storage().await,
            &tx,
            0,
            L1BlockNumber(tester.gateway.block_number("").await.unwrap().as_u32()),
        )
        .await
        .unwrap();

    // check that we sent something and stored it in the db
    assert_eq!(tester.gateway.sent_txs.read().unwrap().len(), 1);
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .len(),
        1
    );

    // mine the transaction but don't have enough confirmations yet
    tester
        .gateway
        .execute_tx(hash, true, EthSenderTester::WAIT_CONFIRMATIONS - 1)?;

    let to_resend = tester
        .manager
        .monitor_inflight_transactions(
            &mut tester.storage().await,
            tester.get_block_numbers().await,
        )
        .await?;

    // check that transaction is still considered inflight
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .len(),
        1
    );

    // also check that we didn't try to resend it
    assert!(to_resend.is_none());

    Ok(())
}

#[db_test]
async fn three_scenarios(connection_pool: ConnectionPool) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(connection_pool.clone(), vec![100; 100], false).await;
    let mut hashes = vec![];

    for _ in 0..3 {
        let tx = tester
            .aggregator
            .save_eth_tx(&mut tester.storage().await, &DUMMY_OPERATION)
            .await
            .unwrap();

        let hash = tester
            .manager
            .send_eth_tx(
                &mut tester.storage().await,
                &tx,
                0,
                L1BlockNumber(tester.gateway.block_number("").await.unwrap().as_u32()),
            )
            .await
            .unwrap();

        hashes.push(hash);
    }

    // check that we sent something
    assert_eq!(tester.gateway.sent_txs.read().unwrap().len(), 3);

    // mined & confirmed
    tester
        .gateway
        .execute_tx(hashes[0], true, EthSenderTester::WAIT_CONFIRMATIONS)?;

    // mined but not confirmed
    tester
        .gateway
        .execute_tx(hashes[1], true, EthSenderTester::WAIT_CONFIRMATIONS - 1)?;

    let (to_resend, _) = tester
        .manager
        .monitor_inflight_transactions(
            &mut tester.storage().await,
            tester.get_block_numbers().await,
        )
        .await?
        .expect("we should be trying to resend the last tx");

    // check that last 2 transactions are still considered inflight
    assert_eq!(
        tester
            .storage()
            .await
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .len(),
        2
    );

    // last sent transaction has nonce == 2, because they start from 0
    assert_eq!(to_resend.nonce.0, 2);

    Ok(())
}

#[should_panic(expected = "We can't operate after tx fail")]
#[db_test]
async fn failed_eth_tx(connection_pool: ConnectionPool) {
    let mut tester = EthSenderTester::new(connection_pool.clone(), vec![100; 100], false).await;

    let tx = tester
        .aggregator
        .save_eth_tx(&mut tester.storage().await, &DUMMY_OPERATION)
        .await
        .unwrap();

    let hash = tester
        .manager
        .send_eth_tx(
            &mut tester.storage().await,
            &tx,
            0,
            L1BlockNumber(tester.gateway.block_number("").await.unwrap().as_u32()),
        )
        .await
        .unwrap();

    // fail this tx
    tester
        .gateway
        .execute_tx(hash, false, EthSenderTester::WAIT_CONFIRMATIONS)
        .unwrap();
    tester
        .manager
        .monitor_inflight_transactions(
            &mut tester.storage().await,
            tester.get_block_numbers().await,
        )
        .await
        .unwrap();
}

fn block_metadata(header: &L1BatchHeader) -> BlockWithMetadata {
    BlockWithMetadata {
        header: header.clone(),
        metadata: BlockMetadata {
            root_hash: Default::default(),
            rollup_last_leaf_index: 0,
            merkle_root_hash: Default::default(),
            initial_writes_compressed: vec![],
            repeated_writes_compressed: vec![],
            commitment: Default::default(),
            l2_l1_messages_compressed: vec![],
            l2_l1_merkle_root: Default::default(),
            block_meta_params: BlockMetaParameters {
                zkporter_is_available: false,
                bootloader_code_hash: Default::default(),
                default_aa_code_hash: Default::default(),
            },
            aux_data_hash: Default::default(),
            meta_parameters_hash: Default::default(),
            pass_through_data_hash: Default::default(),
        },
        factory_deps: vec![],
    }
}

#[db_test]
async fn correct_order_for_confirmations(connection_pool: ConnectionPool) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(connection_pool, vec![100; 100], true).await;
    let zero_block = insert_block(&mut tester, L1BatchNumber(0)).await;
    let first_block = insert_block(&mut tester, L1BatchNumber(1)).await;
    let second_block = insert_block(&mut tester, L1BatchNumber(2)).await;
    commit_block(&mut tester, zero_block.clone(), first_block.clone(), true).await;
    proof_block(&mut tester, zero_block.clone(), first_block.clone(), true).await;
    execute_blocks(&mut tester, vec![first_block.clone()], true).await;
    commit_block(&mut tester, first_block.clone(), second_block.clone(), true).await;
    proof_block(&mut tester, first_block.clone(), second_block.clone(), true).await;

    let blocks = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_blocks(45, None)
        .await;
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].header.number.0, 2);

    execute_blocks(&mut tester, vec![second_block.clone()], true).await;
    let blocks = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_blocks(45, None)
        .await;
    assert_eq!(blocks.len(), 0);
    Ok(())
}

#[db_test]
async fn skipped_block_at_the_start(connection_pool: ConnectionPool) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(connection_pool, vec![100; 100], true).await;
    let zero_block = insert_block(&mut tester, L1BatchNumber(0)).await;
    let first_block = insert_block(&mut tester, L1BatchNumber(1)).await;
    let second_block = insert_block(&mut tester, L1BatchNumber(2)).await;
    commit_block(&mut tester, zero_block.clone(), first_block.clone(), true).await;
    proof_block(&mut tester, zero_block.clone(), first_block.clone(), true).await;
    execute_blocks(&mut tester, vec![first_block.clone()], true).await;
    commit_block(&mut tester, first_block.clone(), second_block.clone(), true).await;
    proof_block(&mut tester, first_block.clone(), second_block.clone(), true).await;
    execute_blocks(&mut tester, vec![second_block.clone()], true).await;

    let third_block = insert_block(&mut tester, L1BatchNumber(3)).await;
    let fourth_block = insert_block(&mut tester, L1BatchNumber(4)).await;
    // DO NOT CONFIRM THIRD BLOCK
    let third_block_commit_tx_hash = commit_block(
        &mut tester,
        second_block.clone(),
        third_block.clone(),
        false,
    )
    .await;

    proof_block(&mut tester, second_block.clone(), third_block.clone(), true).await;
    commit_block(&mut tester, third_block.clone(), fourth_block.clone(), true).await;
    proof_block(&mut tester, third_block.clone(), fourth_block.clone(), true).await;
    let blocks = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_blocks(45, Some(unix_timestamp_ms()))
        .await;
    assert_eq!(blocks.len(), 2);

    confirm_tx(&mut tester, third_block_commit_tx_hash).await;
    let blocks = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_blocks(45, Some(unix_timestamp_ms()))
        .await;
    assert_eq!(blocks.len(), 2);
    Ok(())
}

#[db_test]
async fn skipped_block_in_the_middle(connection_pool: ConnectionPool) -> anyhow::Result<()> {
    let mut tester = EthSenderTester::new(connection_pool, vec![100; 100], true).await;
    let zero_block = insert_block(&mut tester, L1BatchNumber(0)).await;
    let first_block = insert_block(&mut tester, L1BatchNumber(1)).await;
    let second_block = insert_block(&mut tester, L1BatchNumber(2)).await;
    commit_block(&mut tester, zero_block.clone(), first_block.clone(), true).await;
    proof_block(&mut tester, zero_block.clone(), first_block.clone(), true).await;
    execute_blocks(&mut tester, vec![first_block.clone()], true).await;
    commit_block(&mut tester, first_block.clone(), second_block.clone(), true).await;
    proof_block(&mut tester, first_block.clone(), second_block.clone(), true).await;

    let third_block = insert_block(&mut tester, L1BatchNumber(3)).await;
    let fourth_block = insert_block(&mut tester, L1BatchNumber(4)).await;
    // DO NOT CONFIRM THIRD BLOCK
    let third_block_commit_tx_hash = commit_block(
        &mut tester,
        second_block.clone(),
        third_block.clone(),
        false,
    )
    .await;

    proof_block(&mut tester, second_block.clone(), third_block.clone(), true).await;
    commit_block(&mut tester, third_block.clone(), fourth_block.clone(), true).await;
    proof_block(&mut tester, third_block.clone(), fourth_block.clone(), true).await;
    let blocks = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_blocks(45, None)
        .await;
    // We should return all block including third block
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].header.number.0, 2);

    confirm_tx(&mut tester, third_block_commit_tx_hash).await;
    let blocks = tester
        .storage()
        .await
        .blocks_dal()
        .get_ready_for_execute_blocks(45, None)
        .await;
    assert_eq!(blocks.len(), 3);
    Ok(())
}

async fn insert_block(tester: &mut EthSenderTester, number: L1BatchNumber) -> L1BatchHeader {
    let mut block = L1BatchHeader::new(
        number,
        0,
        Address::zero(),
        BaseSystemContractsHashes {
            bootloader: Default::default(),
            default_aa: Default::default(),
        },
    );
    block.is_finished = true;
    // save block to the database
    tester
        .storage()
        .await
        .blocks_dal()
        .insert_l1_batch(&block, Default::default())
        .await;
    tester
        .storage()
        .await
        .blocks_dal()
        .save_blocks_metadata(
            block.number,
            &BlockMetadata {
                root_hash: Default::default(),
                rollup_last_leaf_index: 0,
                merkle_root_hash: Default::default(),
                initial_writes_compressed: vec![],
                repeated_writes_compressed: vec![],
                commitment: Default::default(),
                l2_l1_messages_compressed: vec![],
                l2_l1_merkle_root: Default::default(),
                block_meta_params: BlockMetaParameters {
                    zkporter_is_available: false,
                    bootloader_code_hash: Default::default(),
                    default_aa_code_hash: Default::default(),
                },
                aux_data_hash: Default::default(),
                meta_parameters_hash: Default::default(),
                pass_through_data_hash: Default::default(),
            },
            Default::default(),
        )
        .await;
    block
}

async fn execute_blocks(
    tester: &mut EthSenderTester,
    blocks: Vec<L1BatchHeader>,
    confirm: bool,
) -> H256 {
    let operation = AggregatedOperation::ExecuteBlocks(BlocksExecuteOperation {
        blocks: blocks.iter().map(block_metadata).collect(),
    });
    send_operation(tester, operation, confirm).await
}

async fn proof_block(
    tester: &mut EthSenderTester,
    last_committed_block: L1BatchHeader,
    block: L1BatchHeader,
    confirm: bool,
) -> H256 {
    let operation = AggregatedOperation::PublishProofBlocksOnchain(BlocksProofOperation {
        prev_block: block_metadata(&last_committed_block),
        blocks: vec![block_metadata(&block)],
        proofs: vec![],
        should_verify: false,
    });
    send_operation(tester, operation, confirm).await
}

async fn commit_block(
    tester: &mut EthSenderTester,
    last_committed_block: L1BatchHeader,
    block: L1BatchHeader,
    confirm: bool,
) -> H256 {
    let operation = AggregatedOperation::CommitBlocks(BlocksCommitOperation {
        last_committed_block: block_metadata(&last_committed_block),
        blocks: vec![block_metadata(&block)],
    });
    send_operation(tester, operation, confirm).await
}

async fn send_operation(
    tester: &mut EthSenderTester,
    aggregated_operation: AggregatedOperation,
    confirm: bool,
) -> H256 {
    let tx = tester
        .aggregator
        .save_eth_tx(&mut tester.storage().await, &aggregated_operation)
        .await
        .unwrap();

    let hash = tester
        .manager
        .send_eth_tx(
            &mut tester.storage().await,
            &tx,
            0,
            L1BlockNumber(tester.gateway.block_number("").await.unwrap().as_u32()),
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
        .execute_tx(hash, true, EthSenderTester::WAIT_CONFIRMATIONS)
        .unwrap();

    tester
        .manager
        .monitor_inflight_transactions(
            &mut tester.storage().await,
            tester.get_block_numbers().await,
        )
        .await
        .unwrap();
}
