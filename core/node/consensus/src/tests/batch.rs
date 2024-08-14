use test_casing::{test_casing, Product};
use zksync_concurrency::{ctx, scope};
use zksync_consensus_roles::{
    validator,
};
use zksync_types::{L1BatchNumber, ProtocolVersionId};
use super::{FROM_SNAPSHOT, VERSIONS};
use crate::{
    storage::{ConnectionPool},
    testonly,
};

#[test_casing(4, Product((FROM_SNAPSHOT,VERSIONS)))]
#[tokio::test]
async fn test_connection_get_batch(from_snapshot: bool, version: ProtocolVersionId) {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let pool = ConnectionPool::test(from_snapshot, version).await;

    // Fill storage with unsigned L2 blocks and L1 batches in a way that the
    // last L1 batch is guaranteed to have some L2 blocks executed in it.
    scope::run!(ctx, |ctx, s| async {
        // Start state keeper.
        let (mut sk, runner) = testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run(ctx));

        for _ in 0..3 {
            for _ in 0..2 {
                sk.push_random_block(rng).await;
            }
            sk.seal_batch().await;
        }
        sk.push_random_block(rng).await;

        pool.wait_for_payload(ctx, sk.last_block()).await?;

        Ok(())
    })
    .await
    .unwrap();

    // Now we can try to retrieve the batch.
    scope::run!(ctx, |ctx, _s| async {
        let mut conn = pool.connection(ctx).await?;
        let batches = conn.batches_range(ctx).await?;
        let last = batches.last.expect("last is set");
        let (min, max) = conn
            .get_l2_block_range_of_l1_batch(ctx, last)
            .await?
            .unwrap();

        let last_batch = conn
            .get_batch(ctx, last)
            .await?
            .expect("last batch can be retrieved");

        assert_eq!(
            last_batch.payloads.len(),
            (max.0 - min.0) as usize,
            "all block payloads present"
        );

        let first_payload = last_batch
            .payloads
            .first()
            .expect("last batch has payloads");

        let want_payload = conn.payload(ctx, min).await?.expect("payload is in the DB");
        let want_payload = want_payload.encode();

        assert_eq!(
            first_payload, &want_payload,
            "first payload is the right number"
        );

        anyhow::Ok(())
    })
    .await
    .unwrap();
}

/// Tests that generated L1 batch witnesses can be verified successfully.
/// TODO: add tests for verification failures.
#[test_casing(2, VERSIONS)]
#[tokio::test]
async fn test_batch_witness(version: ProtocolVersionId) {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::from_genesis(version).await;
        let (mut node, runner) = testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run_real(ctx));

        tracing::info!("analyzing storage");
        {
            let mut conn = pool.connection(ctx).await.unwrap();
            let mut n = validator::BlockNumber(0);
            while let Some(p) = conn.payload(ctx, n).await? {
                tracing::info!("block[{n}] = {p:?}");
                n = n + 1;
            }
        }

        // Seal a bunch of batches.
        node.push_random_blocks(rng, 10).await;
        node.seal_batch().await;
        pool.wait_for_batch(ctx, node.last_sealed_batch()).await?;
        // We can verify only 2nd batch onward, because
        // batch witness verifies parent of the last block of the
        // previous batch (and 0th batch contains only 1 block).
        for n in 2..=node.last_sealed_batch().0 {
            let n = L1BatchNumber(n);
            let batch_with_witness = node.load_batch_with_witness(ctx, n).await?;
            let commit = node.load_batch_commit(ctx, n).await?;
            batch_with_witness.verify(&commit)?;
        }
        Ok(())
    })
    .await
    .unwrap();
}
