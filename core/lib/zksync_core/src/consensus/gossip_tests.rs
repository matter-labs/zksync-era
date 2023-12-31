//! Tests for consensus adapters for EN synchronization logic.
use super::*;

use std::ops;

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use zksync_concurrency::{ctx, scope, testonly::abort_on_panic, time};
use zksync_consensus_executor::testonly::ValidatorNode;
use zksync_consensus_roles::validator;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{api::en::SyncBlock, L1BatchNumber, MiniblockNumber, H256};
use crate::consensus::storage::CtxStorage;

use crate::{
    sync_layer::{
        sync_action::SyncAction,
        ActionQueue,
    },
};

const CLOCK_SPEEDUP: i64 = 20;
const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50 * CLOCK_SPEEDUP);

#[tokio::test(flavor = "multi_thread")]
async fn test_backfill() {
    const OPERATOR_ADDRESS: Address = Address::repeat_byte(17);
    const GENESIS_BLOCK: validator::BlockNumber = validator::BlockNumber(5);

    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let pool = ConnectionPool::test_pool().await;

    scope::run!(ctx, |ctx, s| async {
        // Start state keeper.
        let (mut sk, sk_runner) = testonly::StateKeeperHandle::new(OPERATOR_ADDRESS);
        s.spawn_bg(sk_runner.run(ctx, &pool));

        // Populate storage with a bunch of blocks.
        sk.push_random_blocks(rng, 5).await;
        sk.sync(ctx, &pool).await?;
        let validator_cfg = ValidatorNode::for_single_validator(rng);
        let full_node_cfg = validator_cfg.connect_full_node(rng);
        let validators = validator_cfg.node.validators.clone();
        let validator_cfg = Config {
            executor: validator_cfg.node,
            validator: validator_cfg.validator,
            operator_address: OPERATOR_ADDRESS,
        };
        s.spawn_bg(validator_cfg.run(ctx, pool.clone()));
        sk.sync_consensus(ctx, &pool).await?;
        sk.push_random_blocks(rng, 7).await;
        sk.sync_consensus(ctx, &pool).await?;

        //
        let full_node_cfg = FetcherConfig {
            executor: full_node_cfg,
            operator_address: OPERATOR_ADDRESS,
        };

        Ok(())
    })
    .await
    .unwrap();
}
