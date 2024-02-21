use anyhow::Context as _;
use rand::Rng as _;
use tracing::Instrument as _;
use zksync_concurrency::{ctx, scope};
use zksync_consensus_executor as executor;
use zksync_consensus_network as network;
use zksync_consensus_network::testonly::{new_configs, new_fullnode};
use zksync_consensus_roles::validator::testonly::Setup;
use zksync_consensus_storage as storage;
use zksync_consensus_storage::PersistentBlockStore as _;
use zksync_dal::{connection::TestTemplate, ConnectionPool};
use zksync_protobuf::testonly::test_encode_random;

use super::*;

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
        let fork = validator::Fork {
            number: validator::ForkNumber(rng.gen()),
            first_block: validator::BlockNumber(4),
            first_parent: None,
        };
        let mut setup = Setup::new_with_fork(rng, 3, fork.clone());
        let mut conn = sk.store.access(ctx).await.wrap("access()")?;
        conn.try_update_genesis(ctx, &setup.genesis)
            .await
            .wrap("try_update_genesis()")?;
        for i in fork.first_block.0..sk.last_block().next().0 {
            let payload = conn
                .payload(ctx, validator::BlockNumber(i))
                .await
                .wrap(i)?
                .context("payload not found")?
                .encode();
            setup.push_block(payload);
        }
        Ok(setup.blocks.clone())
    })
    .await
    .unwrap();

    // Insert blocks one by one and check the storage state.
    for (i, block) in want.iter().enumerate() {
        let store = Store(pool.clone()).into_block_store();
        store.store_next_block(ctx, block).await.unwrap();
        assert_eq!(want[..i + 1], storage::testonly::dump(ctx, &store).await);
    }
}

fn executor_config(cfg: &network::Config) -> executor::Config {
    executor::Config {
        server_addr: *cfg.server_addr,
        public_addr: cfg.public_addr,
        max_payload_size: usize::MAX,
        node_key: cfg.gossip.key.clone(),
        gossip_dynamic_inbound_limit: cfg.gossip.dynamic_inbound_limit,
        gossip_static_inbound: cfg.gossip.static_inbound.clone(),
        gossip_static_outbound: cfg.gossip.static_outbound.clone(),
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
    let setup = Setup::new(rng, 1);
    let cfgs = new_configs(rng, &setup, 0);

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

        // Restart consensus actor a couple times, making it process a bunch of blocks each time.
        for iteration in 0..3 {
            scope::run!(ctx, |ctx, s| async {
                // Start consensus actor (in the first iteration it will select a genesis block and
                // store a cert for it).
                let cfg = MainNodeConfig {
                    executor: executor_config(&cfgs[0]),
                    validator_key: setup.keys[0].clone(),
                };
                s.spawn_bg(cfg.run(ctx, sk.store.clone()));
                sk.store
                    .wait_for_certificate(ctx, sk.last_block())
                    .await
                    .context("wait_for_certificate(<1st phase>)")?;

                // Generate couple more blocks and wait for consensus to catch up.
                sk.push_random_blocks(rng, 3).await;
                sk.store
                    .wait_for_certificate(ctx, sk.last_block())
                    .await
                    .context("wait_for_certificate(<2nd phase>)")?;

                // Synchronously produce blocks one by one, and wait for consensus.
                for _ in 0..2 {
                    sk.push_random_blocks(rng, 1).await;
                    sk.store
                        .wait_for_certificate(ctx, sk.last_block())
                        .await
                        .context("wait_for_certificate(<3rd phase>)")?;
                }

                sk.store
                    .wait_for_blocks_and_verify(ctx, sk.last_block())
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

// Test running a validator node and a couple of full nodes.
// Validator is producing signed blocks and fetchers are expected to fetch
// them directly or indirectly.
#[tokio::test(flavor = "multi_thread")]
async fn test_full_nodes() {
    const NODES: usize = 2;

    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);
    let validator_cfgs = new_configs(rng, &setup, 0);

    // topology:
    // validator <-> node <-> node <-> ...
    let mut node_cfgs = vec![];
    for _ in 0..NODES {
        node_cfgs.push(new_fullnode(
            rng,
            &node_cfgs.last().unwrap_or(&validator_cfgs[0]),
        ));
    }

    // Run validator and fetchers in parallel.
    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test_pool().await;
        let (mut validator, runner) = testonly::StateKeeper::new(pool).await?;
        s.spawn_bg(async {
            runner
                .run(ctx)
                .instrument(tracing::info_span!("validator"))
                .await
                .context("validator")
        });
        // Generate a couple of blocks, before initializing consensus genesis.
        validator.push_random_blocks(rng, 5).await;
        validator.wait_for_miniblocks(ctx).await.unwrap();

        // Run validator.
        let cfg = MainNodeConfig {
            executor: executor_config(&validator_cfgs[0]),
            validator_key: setup.keys[0].clone(),
        };
        s.spawn_bg(cfg.run(ctx, validator.store.clone()));

        // Run nodes.
        let mut nodes = vec![];
        for (i, cfg) in node_cfgs.iter().enumerate() {
            let i = ctx::NoCopy(i);
            let pool = ConnectionPool::test_pool().await;
            let (node, runner) = testonly::StateKeeper::new(pool).await?;
            nodes.push(node.store.clone());
            s.spawn_bg(async {
                let i = i;
                runner
                    .run(ctx)
                    .instrument(tracing::info_span!("node", i = *i))
                    .await
                    .with_context(|| format!("node{}", *i))
            });
            let fetcher = Fetcher {
                config: executor_config(cfg),
                client: validator.store.client(),
                sync_state: node.sync_state,
            };
            s.spawn_bg(fetcher.run(ctx, node.store, node.actions_sender));
        }

        // Make validator produce blocks and wait for fetchers to get them.
        // Note that block from before and after genesis have to be fetched.
        validator.push_random_blocks(rng, 5).await;
        let want_last = validator.last_block();
        let want = validator
            .store
            .wait_for_blocks_and_verify(ctx, want_last)
            .await?;
        for node in &nodes {
            assert_eq!(want, node.wait_for_blocks_and_verify(ctx, want_last).await?);
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
    let setup = Setup::new(rng, 1);
    let validator_cfgs = new_configs(rng, &setup, 0);
    let cfg = MainNodeConfig {
        executor: executor_config(&validator_cfgs[0]),
        validator_key: setup.keys[0].clone(),
    };

    // Create an initial database snapshot, which contains some blocks: some with certs, some
    // without.
    let pool = scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test_pool().await;
        let (mut sk, runner) = testonly::StateKeeper::new(pool.clone()).await?;
        s.spawn_bg(runner.run(ctx));

        // Some blocks with certs.
        scope::run!(ctx, |ctx, s| async {
            s.spawn_bg(cfg.clone().run(ctx, sk.store.clone()));
            sk.push_random_blocks(rng, 5).await;
            sk.store.wait_for_certificate(ctx, sk.last_block()).await?;
            Ok(())
        })
        .await?;

        // Some blocks without certs.
        sk.push_random_blocks(rng, 5).await;
        sk.wait_for_miniblocks(ctx).await?;
        Ok(pool)
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
        s.spawn_bg(cfg.run(ctx, validator.store.clone()));

        // Run fetcher.
        let pool = template.create_db(4).await?.build().await?;
        let (fetcher, runner) = testonly::StateKeeper::new(pool).await?;
        let store = fetcher.store.clone();
        s.spawn_bg(runner.run(ctx));
        let actions = fetcher.actions_sender;
        let fetcher = Fetcher {
            config: executor_config(&new_fullnode(rng, &validator_cfgs[0])),
            client: validator.store.client(),
            sync_state: fetcher.sync_state,
        };
        s.spawn_bg(fetcher.run(ctx, store.clone(), actions));

        // Make validator produce new blocks and
        // wait for the fetcher to get both the missing certs and the new blocks.
        validator.push_random_blocks(rng, 5).await;
        store
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
