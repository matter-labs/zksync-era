use std::ops::Range;

use anyhow::Context as _;
use tracing::Instrument as _;
use zksync_concurrency::{ctx, scope};
use zksync_consensus_executor::testonly::{connect_full_node, ValidatorNode};
use zksync_consensus_storage as storage;
use zksync_consensus_storage::PersistentBlockStore as _;
use zksync_consensus_utils::no_copy::NoCopy;
use zksync_dal::{connection::TestTemplate, ConnectionPool};
use zksync_protobuf::testonly::test_encode_random;

use super::*;
use crate::consensus::storage::CtxStorage;

async fn make_blocks(
    ctx: &ctx::Ctx,
    pool: &ConnectionPool,
    mut range: Range<validator::BlockNumber>,
) -> ctx::Result<Vec<validator::FinalBlock>> {
    let rng = &mut ctx.rng();
    let mut storage = CtxStorage::access(ctx, pool).await.wrap("access()")?;
    let mut blocks: Vec<validator::FinalBlock> = vec![];
    while !range.is_empty() {
        let payload = storage
            .payload(ctx, range.start)
            .await
            .wrap(range.start)?
            .context("payload not found")?
            .encode();
        let header = match blocks.last().as_ref() {
            Some(parent) => validator::BlockHeader::new(parent.header(), payload.hash()),
            None => validator::BlockHeader::genesis(payload.hash(), range.start),
        };
        blocks.push(validator::FinalBlock {
            payload,
            justification: validator::testonly::make_justification(
                rng,
                &header,
                validator::ProtocolVersion::EARLIEST,
            ),
        });
        range.start = range.start.next();
    }
    Ok(blocks)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_validator_block_store() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let pool = ConnectionPool::test_pool().await;

    // Fill storage with unsigned miniblocks.
    // Fetch a suffix of blocks that we will generate (fake) certs for.
    let want = scope::run!(ctx, |ctx, s| async {
        // Start state keeper.
        let (mut sk, runner) = testonly::StateKeeper::new(pool.clone()).await?;
        s.spawn_bg(runner.run(ctx));
        sk.push_random_blocks(rng, 10).await;
        sk.wait_for_miniblocks(ctx).await?;
        let range = Range {
            start: validator::BlockNumber(4),
            end: sk.last_block(),
        };
        make_blocks(ctx, &sk.pool, range)
            .await
            .context("make_blocks")
    })
    .await
    .unwrap();

    // Insert blocks one by one and check the storage state.
    for (i, block) in want.iter().enumerate() {
        let store = Store::new(pool.clone()).into_block_store();
        store.store_next_block(ctx, block).await.unwrap();
        assert_eq!(want[..i + 1], storage::testonly::dump(ctx, &store).await);
    }
}

// In the current implementation, consensus certificates are created asynchronously
// for the miniblocks constructed by the StateKeeper. This means that consensus actor
// is effectively just back filling the consensus certificates for the miniblocks in storage.
#[tokio::test(flavor = "multi_thread")]
async fn test_validator() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();

    scope::run!(ctx, |ctx, s| async {
        // Start state keeper.
        let pool = ConnectionPool::test_pool().await;
        let (mut sk, runner) = testonly::StateKeeper::new(pool).await?;
        s.spawn_bg(runner.run(ctx));

        // Populate storage with a bunch of blocks.
        sk.push_random_blocks(rng, 5).await;
        sk.wait_for_miniblocks(ctx)
            .await
            .context("sk.wait_for_miniblocks(<1st phase>)")?;

        let cfg = ValidatorNode::new(&mut ctx.rng());
        let validators = cfg.node.validators.clone();

        // Restart consensus actor a couple times, making it process a bunch of blocks each time.
        for iteration in 0..3 {
            scope::run!(ctx, |ctx, s| async {
                // Start consensus actor (in the first iteration it will select a genesis block and
                // store a cert for it).
                let cfg = MainNodeConfig {
                    executor: cfg.node.clone(),
                    validator: cfg.validator.clone(),
                };
                s.spawn_bg(cfg.run(ctx, sk.pool.clone()));
                sk.store()
                    .wait_for_certificate(ctx, sk.last_block())
                    .await
                    .context("wait_for_certificate(<1st phase>)")?;

                // Generate couple more blocks and wait for consensus to catch up.
                sk.push_random_blocks(rng, 3).await;
                sk.store()
                    .wait_for_certificate(ctx, sk.last_block())
                    .await
                    .context("wait_for_certificate(<2nd phase>)")?;

                // Synchronously produce blocks one by one, and wait for consensus.
                for _ in 0..2 {
                    sk.push_random_blocks(rng, 1).await;
                    sk.store()
                        .wait_for_certificate(ctx, sk.last_block())
                        .await
                        .context("wait_for_certificate(<3rd phase>)")?;
                }

                sk.store()
                    .wait_for_blocks_and_verify(ctx, &validators, sk.last_block())
                    .await
                    .context("wait_for_blocks_and_verify()")?;
                Ok(())
            })
            .await
            .context(iteration)?;
        }
        Ok(())
    })
    .await
    .unwrap();
}

// Test running a validator node and a couple of full nodes (aka fetchers).
// Validator is producing signed blocks and fetchers are expected to fetch
// them directly or indirectly.
#[tokio::test(flavor = "multi_thread")]
async fn test_fetcher() {
    const FETCHERS: usize = 2;

    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();

    // topology:
    // validator <-> fetcher <-> fetcher <-> ...
    let cfg = ValidatorNode::new(rng);
    let validators = cfg.node.validators.clone();
    let mut cfg = MainNodeConfig {
        executor: cfg.node,
        validator: cfg.validator,
    };
    let mut fetcher_cfgs = vec![connect_full_node(rng, &mut cfg.executor)];
    while fetcher_cfgs.len() < FETCHERS {
        let cfg = connect_full_node(rng, fetcher_cfgs.last_mut().unwrap());
        fetcher_cfgs.push(cfg);
    }
    let fetcher_cfgs: Vec<_> = fetcher_cfgs
        .into_iter()
        .map(|executor| FetcherConfig { executor })
        .collect();

    // Create an initial database snapshot, which contains a cert for genesis block.
    let pool = scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test_pool().await;
        let (mut sk, runner) = testonly::StateKeeper::new(pool).await?;
        s.spawn_bg(runner.run(ctx));
        s.spawn_bg(cfg.clone().run(ctx, sk.pool.clone()));
        sk.push_random_blocks(rng, 5).await;
        sk.store()
            .wait_for_certificate(ctx, sk.last_block())
            .await?;
        Ok(sk.pool)
    })
    .await
    .unwrap();
    let template = TestTemplate::freeze(pool).await.unwrap();

    // Run validator and fetchers in parallel.
    scope::run!(ctx, |ctx, s| async {
        // Run validator.
        let pool = template.create_db(4).await?.build().await?;
        let (mut validator, runner) = testonly::StateKeeper::new(pool).await?;
        s.spawn_bg(async {
            runner
                .run(ctx)
                .instrument(tracing::info_span!("validator"))
                .await
                .context("validator")
        });
        s.spawn_bg(cfg.run(ctx, validator.pool.clone()));

        // Run fetchers.
        let mut fetchers = vec![];
        for (i, cfg) in fetcher_cfgs.into_iter().enumerate() {
            let i = NoCopy::from(i);
            let pool = template.create_db(4).await?.build().await?;
            let (fetcher, runner) = testonly::StateKeeper::new(pool).await?;
            fetchers.push(fetcher.store());
            s.spawn_bg(async {
                let i = i;
                runner
                    .run(ctx)
                    .instrument(tracing::info_span!("fetcher", i = *i))
                    .await
                    .with_context(|| format!("fetcher{}", *i))
            });
            s.spawn_bg(cfg.run(ctx, fetcher.pool, fetcher.actions_sender));
        }

        // Make validator produce blocks and wait for fetchers to get them.
        validator.push_random_blocks(rng, 5).await;
        let want_last = validator.last_block();
        let want = validator
            .store()
            .wait_for_blocks_and_verify(ctx, &validators, want_last)
            .await?;
        for fetcher in &fetchers {
            assert_eq!(
                want,
                fetcher
                    .wait_for_blocks_and_verify(ctx, &validators, want_last)
                    .await?
            );
        }
        Ok(())
    })
    .await
    .unwrap();
}

// Test fetcher back filling missing certs.
#[tokio::test(flavor = "multi_thread")]
async fn test_fetcher_backfill_certs() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();

    let cfg = ValidatorNode::new(rng);
    let mut cfg = MainNodeConfig {
        executor: cfg.node,
        validator: cfg.validator,
    };
    let fetcher_cfg = FetcherConfig {
        executor: connect_full_node(rng, &mut cfg.executor),
    };

    // Create an initial database snapshot, which contains some blocks: some with certs, some
    // without.
    let pool = scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test_pool().await;
        let (mut sk, runner) = testonly::StateKeeper::new(pool).await?;
        s.spawn_bg(runner.run(ctx));

        // Some blocks with certs.
        scope::run!(ctx, |ctx, s| async {
            s.spawn_bg(cfg.clone().run(ctx, sk.pool.clone()));
            sk.push_random_blocks(rng, 5).await;
            sk.store()
                .wait_for_certificate(ctx, sk.last_block())
                .await?;
            Ok(())
        })
        .await?;

        // Some blocks without certs.
        sk.push_random_blocks(rng, 5).await;
        sk.wait_for_miniblocks(ctx).await?;
        Ok(sk.pool)
    })
    .await
    .unwrap();
    let template = TestTemplate::freeze(pool).await.unwrap();

    // Run validator and fetchers in parallel.
    scope::run!(ctx, |ctx, s| async {
        // Run validator.
        let pool = template.create_db(4).await?.build().await?;
        let (mut validator, runner) = testonly::StateKeeper::new(pool).await?;
        s.spawn_bg(runner.run(ctx));
        s.spawn_bg(cfg.run(ctx, validator.pool.clone()));

        // Run fetcher.
        let pool = template.create_db(4).await?.build().await?;
        let (fetcher, runner) = testonly::StateKeeper::new(pool).await?;
        let fetcher_store = fetcher.store();
        s.spawn_bg(runner.run(ctx));
        s.spawn_bg(fetcher_cfg.run(ctx, fetcher.pool, fetcher.actions_sender));

        // Make validator produce new blocks and
        // wait for the fetcher to get both the missing certs and the new blocks.
        validator.push_random_blocks(rng, 5).await;
        fetcher_store
            .wait_for_certificate(ctx, validator.last_block())
            .await?;
        Ok(())
    })
    .await
    .unwrap();
}

#[test]
fn test_schema_encoding() {
    let ctx = ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    test_encode_random::<config::Config>(rng);
}
