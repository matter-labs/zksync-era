//! Tests for consensus adapters for EN synchronization logic.

use std::ops;

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use zksync_concurrency::{ctx, scope, testonly::abort_on_panic, time};
use zksync_consensus_executor::testonly::FullValidatorConfig;
use zksync_consensus_roles::validator::{self, FinalBlock};
use zksync_consensus_storage::{InMemoryStorage, WriteBlockStore};
use zksync_dal::{blocks_dal::ConsensusBlockFields, ConnectionPool, StorageProcessor};
use zksync_types::{api::en::SyncBlock, L1BatchNumber, MiniblockNumber, H256};

use super::*;
use crate::{
    consensus,
    sync_layer::{
        sync_action::SyncAction,
        tests::{
            mock_l1_batch_hash_computation, run_state_keeper_with_multiple_l1_batches,
            run_state_keeper_with_multiple_miniblocks, StateKeeperHandles, OPERATOR_ADDRESS,
        },
        ActionQueue,
    },
};

const CLOCK_SPEEDUP: i64 = 20;
const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50 * CLOCK_SPEEDUP);

async fn load_sync_block(storage: &mut StorageProcessor<'_>, number: u32) -> SyncBlock {
    storage
        .sync_dal()
        .sync_block(MiniblockNumber(number), OPERATOR_ADDRESS, true)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("no sync block #{number}"))
}

/// Loads a block from the storage and converts it to a `FinalBlock`.
pub(super) async fn load_final_block(
    storage: &mut StorageProcessor<'_>,
    number: u32,
) -> FinalBlock {
    let sync_block = load_sync_block(storage, number).await;
    consensus::sync_block_to_consensus_block(sync_block).unwrap()
}

fn convert_sync_blocks(sync_blocks: Vec<SyncBlock>) -> Vec<FinalBlock> {
    sync_blocks
        .into_iter()
        .map(|sync_block| consensus::sync_block_to_consensus_block(sync_block).unwrap())
        .collect()
}

pub(super) async fn block_payload(
    storage: &mut StorageProcessor<'_>,
    number: u32,
) -> consensus::Payload {
    let sync_block = load_sync_block(storage, number).await;
    consensus::Payload::try_from(sync_block).unwrap()
}

/// Adds consensus information for the specified `count` of miniblocks, starting from the genesis.
pub(super) async fn add_consensus_fields(
    storage: &mut StorageProcessor<'_>,
    validator_key: &validator::SecretKey,
    block_numbers: ops::Range<u32>,
) {
    let mut prev_block_hash = validator::BlockHeaderHash::from_bytes([0; 32]);
    let validator_set = validator::ValidatorSet::new([validator_key.public()]).unwrap();
    for number in block_numbers {
        let payload = block_payload(storage, number).await.encode();
        let block_header = validator::BlockHeader {
            parent: prev_block_hash,
            number: validator::BlockNumber(number.into()),
            payload: payload.hash(),
        };
        let replica_commit = validator::ReplicaCommit {
            protocol_version: validator::ProtocolVersion::EARLIEST,
            view: validator::ViewNumber(number.into()),
            proposal: block_header,
        };
        let replica_commit = validator_key.sign_msg(replica_commit);
        let justification = validator::CommitQC::from(&[replica_commit], &validator_set)
            .expect("Failed creating QC");

        let consensus = ConsensusBlockFields {
            parent: prev_block_hash,
            justification,
        };
        storage
            .blocks_dal()
            .set_miniblock_consensus_fields(MiniblockNumber(number), &consensus)
            .await
            .unwrap();
        prev_block_hash = block_header.hash();
    }
}

/// Creates a genesis block for the consensus with the specified number / payload authored by a single validator.
pub(super) fn create_genesis_block(
    validator_key: &validator::SecretKey,
    number: u64,
    payload: validator::Payload,
) -> FinalBlock {
    let block_header = validator::BlockHeader {
        parent: validator::BlockHeaderHash::from_bytes([0; 32]),
        number: validator::BlockNumber(number),
        payload: payload.hash(),
    };
    let validator_set = validator::ValidatorSet::new([validator_key.public()]).unwrap();
    let replica_commit = validator::ReplicaCommit {
        protocol_version: validator::ProtocolVersion::EARLIEST,
        view: validator::ViewNumber(number),
        proposal: block_header,
    };
    let replica_commit = validator_key.sign_msg(replica_commit);
    let justification =
        validator::CommitQC::from(&[replica_commit], &validator_set).expect("Failed creating QC");
    FinalBlock {
        header: block_header,
        payload,
        justification,
    }
}

pub(super) async fn assert_first_block_actions(actions: &mut ActionQueue) -> Vec<SyncAction> {
    let mut received_actions = vec![];
    while !matches!(received_actions.last(), Some(SyncAction::SealMiniblock(_))) {
        received_actions.push(actions.recv_action().await);
    }
    assert_matches!(
        received_actions.as_slice(),
        [
            SyncAction::OpenBatch {
                number: L1BatchNumber(1),
                timestamp: 1,
                first_miniblock_info: (MiniblockNumber(1), 1),
                ..
            },
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::SealMiniblock(_),
        ]
    );
    received_actions
}

pub(super) async fn assert_second_block_actions(actions: &mut ActionQueue) -> Vec<SyncAction> {
    let mut received_actions = vec![];
    while !matches!(received_actions.last(), Some(SyncAction::SealMiniblock(_))) {
        received_actions.push(actions.recv_action().await);
    }
    assert_matches!(
        received_actions.as_slice(),
        [
            SyncAction::Miniblock {
                number: MiniblockNumber(2),
                timestamp: 2,
                virtual_blocks: 1,
            },
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::SealMiniblock(_),
        ]
    );
    received_actions
}

#[test_casing(4, Product(([false, true], [false, true])))]
#[tokio::test]
async fn syncing_via_gossip_fetcher(delay_first_block: bool, delay_second_block: bool) {
    abort_on_panic();
    let pool = ConnectionPool::test_pool().await;
    let tx_hashes = run_state_keeper_with_multiple_miniblocks(pool.clone()).await;

    let mut storage = pool.access_storage().await.unwrap();
    let genesis_block_payload = block_payload(&mut storage, 0).await.encode();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(CLOCK_SPEEDUP as f64));
    let rng = &mut ctx.rng();
    let mut validator = FullValidatorConfig::for_single_validator(
        rng,
        genesis_block_payload,
        validator::BlockNumber(0),
    );
    let validator_set = validator.node_config.validators.clone();
    let external_node = validator.connect_full_node(rng);

    let genesis_block = validator.node_config.genesis_block.clone();
    add_consensus_fields(&mut storage, &validator.validator_key, 0..3).await;
    let blocks = convert_sync_blocks(reset_storage(storage).await);
    let [first_block, second_block] = blocks.as_slice() else {
        unreachable!("Unexpected blocks in storage: {blocks:?}");
    };
    tracing::trace!("Node storage reset");

    let validator_storage = Arc::new(InMemoryStorage::new(genesis_block));
    if !delay_first_block {
        validator_storage.put_block(ctx, first_block).await.unwrap();
        if !delay_second_block {
            validator_storage
                .put_block(ctx, second_block)
                .await
                .unwrap();
        }
    }

    let (actions_sender, mut actions) = ActionQueue::new();
    let (keeper_actions_sender, keeper_actions) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(pool.clone(), keeper_actions, &[&tx_hashes]).await;
    scope::run!(ctx, |ctx, s| async {
        let validator = Executor::new(
            ctx,
            validator.node_config,
            validator.node_key,
            validator_storage.clone(),
        )
        .await?;
        // ^ We intentionally do not run consensus on the validator node, since it'll produce blocks
        // with payloads that cannot be parsed by the external node.

        s.spawn_bg(validator.run(ctx));
        s.spawn_bg(run_gossip_fetcher_inner(
            ctx,
            pool.clone(),
            actions_sender,
            external_node.node_config,
            external_node.node_key,
            OPERATOR_ADDRESS,
        ));

        if delay_first_block {
            ctx.sleep(POLL_INTERVAL).await?;
            validator_storage.put_block(ctx, first_block).await.unwrap();
            if !delay_second_block {
                validator_storage
                    .put_block(ctx, second_block)
                    .await
                    .unwrap();
            }
        }

        let received_actions = assert_first_block_actions(&mut actions).await;
        // Manually replicate actions to the state keeper.
        keeper_actions_sender.push_actions(received_actions).await;

        if delay_second_block {
            validator_storage
                .put_block(ctx, second_block)
                .await
                .unwrap();
        }

        let received_actions = assert_second_block_actions(&mut actions).await;
        keeper_actions_sender.push_actions(received_actions).await;
        state_keeper
            .wait(|state| state.get_local_block() == MiniblockNumber(2))
            .await;
        Ok(())
    })
    .await
    .unwrap();

    // Check that received blocks have consensus fields persisted.
    let mut storage = pool.access_storage().await.unwrap();
    for number in [1, 2] {
        let block = load_final_block(&mut storage, number).await;
        block.justification.verify(&validator_set, 1).unwrap();
    }
}

/// Returns the removed blocks.
async fn reset_storage(mut storage: StorageProcessor<'_>) -> Vec<SyncBlock> {
    let sealed_miniblock_number = storage
        .blocks_dal()
        .get_sealed_miniblock_number()
        .await
        .unwrap();
    let mut blocks = vec![];
    for number in 1..=sealed_miniblock_number.0 {
        blocks.push(load_sync_block(&mut storage, number).await);
    }

    storage
        .transactions_dal()
        .reset_transactions_state(MiniblockNumber(0))
        .await;
    storage
        .blocks_dal()
        .delete_miniblocks(MiniblockNumber(0))
        .await
        .unwrap();
    storage
        .blocks_dal()
        .delete_l1_batches(L1BatchNumber(0))
        .await
        .unwrap();
    blocks
}

#[test_casing(4, [3, 2, 1, 0])]
#[tokio::test]
async fn syncing_via_gossip_fetcher_with_multiple_l1_batches(initial_block_count: usize) {
    assert!(initial_block_count <= 3);
    abort_on_panic();

    let pool = ConnectionPool::test_pool().await;
    let tx_hashes = run_state_keeper_with_multiple_l1_batches(pool.clone()).await;
    let tx_hashes: Vec<_> = tx_hashes.iter().map(Vec::as_slice).collect();

    let mut storage = pool.access_storage().await.unwrap();
    let genesis_block_payload = block_payload(&mut storage, 0).await.encode();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(CLOCK_SPEEDUP as f64));
    let rng = &mut ctx.rng();
    let mut validator = FullValidatorConfig::for_single_validator(
        rng,
        genesis_block_payload,
        validator::BlockNumber(0),
    );
    let validator_set = validator.node_config.validators.clone();
    let external_node = validator.connect_full_node(rng);

    let genesis_block = validator.node_config.genesis_block.clone();
    add_consensus_fields(&mut storage, &validator.validator_key, 0..4).await;
    let blocks = convert_sync_blocks(reset_storage(storage).await);
    assert_eq!(blocks.len(), 3); // 2 real + 1 fictive blocks
    tracing::trace!("Node storage reset");
    let (initial_blocks, delayed_blocks) = blocks.split_at(initial_block_count);

    let validator_storage = Arc::new(InMemoryStorage::new(genesis_block));
    for block in initial_blocks {
        validator_storage.put_block(ctx, block).await.unwrap();
    }

    let (actions_sender, actions) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(pool.clone(), actions, &tx_hashes).await;
    scope::run!(ctx, |ctx, s| async {
        let validator = Executor::new(
            ctx,
            validator.node_config,
            validator.node_key,
            validator_storage.clone(),
        )
        .await?;

        s.spawn_bg(validator.run(ctx));
        s.spawn_bg(async {
            for block in delayed_blocks {
                ctx.sleep(POLL_INTERVAL).await?;
                validator_storage.put_block(ctx, block).await?;
            }
            Ok(())
        });

        s.spawn_bg(async {
            mock_l1_batch_hash_computation(pool.clone(), 1).await;
            Ok(())
        });
        s.spawn_bg(run_gossip_fetcher_inner(
            ctx,
            pool.clone(),
            actions_sender,
            external_node.node_config,
            external_node.node_key,
            OPERATOR_ADDRESS,
        ));

        state_keeper
            .wait(|state| state.get_local_block() == MiniblockNumber(3))
            .await;
        Ok(())
    })
    .await
    .unwrap();

    // Check that received blocks have consensus fields persisted.
    let mut storage = pool.access_storage().await.unwrap();
    for number in [1, 2, 3] {
        let block = load_final_block(&mut storage, number).await;
        block.validate(&validator_set, 1).unwrap();
    }
}

#[test_casing(2, [1, 2])]
#[tokio::test(flavor = "multi_thread")]
async fn syncing_from_non_zero_block(first_block_number: u32) {
    abort_on_panic();
    let pool = ConnectionPool::test_pool().await;
    let tx_hashes = run_state_keeper_with_multiple_l1_batches(pool.clone()).await;
    let tx_hashes: Vec<_> = tx_hashes.iter().map(Vec::as_slice).collect();

    let mut storage = pool.access_storage().await.unwrap();
    let genesis_block_payload = block_payload(&mut storage, first_block_number)
        .await
        .encode();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(CLOCK_SPEEDUP as f64));
    let rng = &mut ctx.rng();
    let mut validator = FullValidatorConfig::for_single_validator(
        rng,
        genesis_block_payload.clone(),
        validator::BlockNumber(0),
    );
    // Override the genesis block since it has an incorrect block number.
    let genesis_block = create_genesis_block(
        &validator.validator_key,
        first_block_number.into(),
        genesis_block_payload,
    );
    validator.node_config.genesis_block = genesis_block.clone();
    let validator_set = validator.node_config.validators.clone();
    let external_node = validator.connect_full_node(rng);

    add_consensus_fields(
        &mut storage,
        &validator.validator_key,
        first_block_number..4,
    )
    .await;
    let mut initial_blocks = reset_storage(storage).await;
    let delayed_blocks = initial_blocks.split_off(first_block_number as usize);
    assert!(!initial_blocks.is_empty());
    assert!(!delayed_blocks.is_empty());
    let delayed_blocks = convert_sync_blocks(delayed_blocks);

    // Re-insert initial blocks to the storage. This allows to more precisely emulate node syncing
    // (e.g., missing L1 batch relation for the latest blocks).
    insert_sync_blocks(pool.clone(), initial_blocks, &tx_hashes).await;
    tracing::trace!("Re-inserted blocks to node storage");

    let validator_storage = Arc::new(InMemoryStorage::new(genesis_block));
    let tx_hashes = if first_block_number >= 2 {
        &tx_hashes[1..] // Skip transactions in L1 batch #1, since they won't be executed
    } else {
        &tx_hashes
    };
    let (actions_sender, actions) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(pool.clone(), actions, tx_hashes).await;
    scope::run!(ctx, |ctx, s| async {
        let validator = Executor::new(
            ctx,
            validator.node_config,
            validator.node_key,
            validator_storage.clone(),
        )
        .await?;

        s.spawn_bg(async { validator.run(ctx).await.context("validator.run()") });

        s.spawn_bg(async {
            for block in &delayed_blocks {
                ctx.sleep(POLL_INTERVAL).await?;
                validator_storage
                    .put_block(ctx, block)
                    .await
                    .wrap("validator_stroage.put_block()")?;
            }
            Ok(())
        });

        if first_block_number < 2 {
            // L1 batch #1 will be sealed during the state keeper operation; we need to emulate
            // computing metadata for it.
            s.spawn_bg(async {
                ctx.wait(mock_l1_batch_hash_computation(pool.clone(), 1))
                    .await?;
                Ok(())
            });
        }

        s.spawn_bg(async {
            run_gossip_fetcher_inner(
                ctx,
                pool.clone(),
                actions_sender,
                external_node.node_config,
                external_node.node_key,
                OPERATOR_ADDRESS,
            )
            .await
            .context("run_gossip_fetcher_inner()")
        });

        ctx.wait(state_keeper.wait(|state| state.get_local_block() == MiniblockNumber(3)))
            .await?;
        Ok(())
    })
    .await
    .unwrap();

    // Check that received blocks have consensus fields persisted.
    let mut storage = pool.access_storage().await.unwrap();
    for number in first_block_number..4 {
        let block = load_final_block(&mut storage, number).await;
        block.justification.verify(&validator_set, 1).unwrap();
    }
}

async fn insert_sync_blocks(pool: ConnectionPool, blocks: Vec<SyncBlock>, tx_hashes: &[&[H256]]) {
    let expected_block_number = blocks.last().expect("blocks cannot be empty").number;
    let sealed_l1_batches = blocks
        .iter()
        .filter_map(|block| block.last_in_batch.then_some(block.l1_batch_number));
    let sealed_l1_batches: Vec<_> = sealed_l1_batches.collect();

    let mut fetcher = FetcherCursor::new(&mut pool.access_storage().await.unwrap())
        .await
        .unwrap();
    let (actions_sender, actions) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(pool.clone(), actions, tx_hashes).await;
    for block in blocks {
        let block_actions = fetcher.advance(block.try_into().unwrap());
        actions_sender.push_actions(block_actions).await;
    }

    let hash_tasks: Vec<_> = sealed_l1_batches
        .into_iter()
        .map(|l1_batch_number| {
            tokio::spawn(mock_l1_batch_hash_computation(
                pool.clone(),
                l1_batch_number.0,
            ))
        })
        .collect();
    state_keeper
        .wait(|state| state.get_local_block() == expected_block_number)
        .await;
    for hash_task in hash_tasks {
        hash_task.await.unwrap();
    }
}
