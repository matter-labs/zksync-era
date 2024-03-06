use anyhow::Context as _;
use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use test_casing::test_casing;
use tracing::Instrument as _;
use zksync_concurrency::{ctx, scope};
use zksync_config::testonly::{Gen, RandomConfig};
use zksync_consensus_executor as executor;
use zksync_consensus_network as network;
use zksync_consensus_network::testonly::{new_configs, new_fullnode};
use zksync_consensus_roles::{node, validator::testonly::Setup};
use zksync_consensus_storage as storage;
use zksync_consensus_storage::PersistentBlockStore as _;
use zksync_protobuf_config::testonly::{encode_decode, FmtConv};

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_validator_block_store() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let store = testonly::new_store(false).await;

    // Fill storage with unsigned miniblocks.
    // Fetch a suffix of blocks that we will generate (fake) certs for.
    let want = scope::run!(ctx, |ctx, s| async {
        // Start state keeper.
        let (mut sk, runner) = testonly::StateKeeper::new(ctx, store.clone()).await?;
        s.spawn_bg(runner.run(ctx));
        sk.push_random_blocks(rng, 10).await;
        store.wait_for_block(ctx, sk.last_block()).await?;
        let fork = validator::Fork {
            number: validator::ForkNumber(rng.gen()),
            first_block: validator::BlockNumber(4),
            first_parent: None,
        };
        let mut setup = Setup::new_with_fork(rng, 3, fork.clone());
        let mut conn = store.access(ctx).await.wrap("access()")?;
        conn.try_update_genesis(ctx, &setup.genesis)
            .await
            .wrap("try_update_genesis()")?;
        for i in fork.first_block.0..sk.last_block().next().0 {
            let i = validator::BlockNumber(i);
            let payload = conn
                .payload(ctx, i)
                .await
                .wrap(i)?
                .with_context(|| format!("payload for {i:?} not found"))?
                .encode();
            setup.push_block(payload);
        }
        Ok(setup.blocks.clone())
    })
    .await
    .unwrap();

    // Insert blocks one by one and check the storage state.
    for (i, block) in want.iter().enumerate() {
        let store = store.clone().into_block_store();
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
        let store = testonly::new_store(false).await;
        let (mut sk, runner) = testonly::StateKeeper::new(ctx, store.clone()).await?;
        s.spawn_bg(runner.run(ctx));

        // Populate storage with a bunch of blocks.
        sk.push_random_blocks(rng, 5).await;
        store
            .wait_for_block(ctx, sk.last_block())
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
                s.spawn_bg(cfg.run(ctx, store.clone()));
                store
                    .wait_for_certificate(ctx, sk.last_block())
                    .await
                    .context("wait_for_certificate(<1st phase>)")?;

                // Generate couple more blocks and wait for consensus to catch up.
                sk.push_random_blocks(rng, 3).await;
                store
                    .wait_for_certificate(ctx, sk.last_block())
                    .await
                    .context("wait_for_certificate(<2nd phase>)")?;

                // Synchronously produce blocks one by one, and wait for consensus.
                for _ in 0..2 {
                    sk.push_random_blocks(rng, 1).await;
                    store
                        .wait_for_certificate(ctx, sk.last_block())
                        .await
                        .context("wait_for_certificate(<3rd phase>)")?;
                }

                store
                    .wait_for_certificates_and_verify(ctx, sk.last_block())
                    .await
                    .context("wait_for_certificates_and_verify()")?;
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
            node_cfgs.last().unwrap_or(&validator_cfgs[0]),
        ));
    }

    // Run validator and fetchers in parallel.
    scope::run!(ctx, |ctx, s| async {
        let validator_store = testonly::new_store(false).await;
        let (mut validator, runner) =
            testonly::StateKeeper::new(ctx, validator_store.clone()).await?;
        s.spawn_bg(async {
            runner
                .run(ctx)
                .instrument(tracing::info_span!("validator"))
                .await
                .context("validator")
        });
        // Generate a couple of blocks, before initializing consensus genesis.
        validator.push_random_blocks(rng, 5).await;
        validator_store
            .wait_for_block(ctx, validator.last_block())
            .await
            .unwrap();

        // Run validator.
        let cfg = MainNodeConfig {
            executor: executor_config(&validator_cfgs[0]),
            validator_key: setup.keys[0].clone(),
        };
        s.spawn_bg(cfg.run(ctx, validator_store.clone()));

        // Run nodes.
        let mut node_stores = vec![];
        for (i, cfg) in node_cfgs.iter().enumerate() {
            let i = ctx::NoCopy(i);
            let store = testonly::new_store(false).await;
            let (node, runner) = testonly::StateKeeper::new(ctx, store.clone()).await?;
            node_stores.push(store.clone());
            s.spawn_bg(async {
                let i = i;
                runner
                    .run(ctx)
                    .instrument(tracing::info_span!("node", i = *i))
                    .await
                    .with_context(|| format!("node{}", *i))
            });
            s.spawn_bg(node.run_p2p_fetcher(
                ctx,
                validator.connect(ctx).await?,
                executor_config(cfg),
            ));
        }

        // Make validator produce blocks and wait for fetchers to get them.
        // Note that block from before and after genesis have to be fetched.
        validator.push_random_blocks(rng, 5).await;
        let want_last = validator.last_block();
        let want = validator_store
            .wait_for_certificates_and_verify(ctx, want_last)
            .await?;
        for store in &node_stores {
            assert_eq!(
                want,
                store
                    .wait_for_certificates_and_verify(ctx, want_last)
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
async fn test_p2p_fetcher_backfill_certs() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);
    let validator_cfg = new_configs(rng, &setup, 0)[0].clone();
    let node_cfg = executor_config(&new_fullnode(rng, &validator_cfg));

    scope::run!(ctx, |ctx, s| async {
        tracing::info!("Spawn validator.");
        let validator_store = testonly::new_store(false).await;
        let (mut validator, runner) =
            testonly::StateKeeper::new(ctx, validator_store.clone()).await?;
        s.spawn_bg(runner.run(ctx));
        s.spawn_bg(
            MainNodeConfig {
                executor: executor_config(&validator_cfg),
                validator_key: setup.keys[0].clone(),
            }
            .run(ctx, validator_store.clone()),
        );
        let client = validator.connect(ctx).await?;

        let node_store = testonly::new_store(false).await;

        tracing::info!("Run p2p fetcher.");
        scope::run!(ctx, |ctx, s| async {
            let (node, runner) = testonly::StateKeeper::new(ctx, node_store.clone()).await?;
            s.spawn_bg(runner.run(ctx));
            s.spawn_bg(node.run_p2p_fetcher(ctx, client.clone(), node_cfg.clone()));
            validator.push_random_blocks(rng, 3).await;
            node_store
                .wait_for_certificate(ctx, validator.last_block())
                .await?;
            Ok(())
        })
        .await
        .unwrap();

        tracing::info!("Run centralized fetcher.");
        scope::run!(ctx, |ctx, s| async {
            let (node, runner) = testonly::StateKeeper::new(ctx, node_store.clone()).await?;
            s.spawn_bg(runner.run(ctx));
            s.spawn_bg(node.run_centralized_fetcher(ctx, client.clone()));
            validator.push_random_blocks(rng, 3).await;
            node_store
                .wait_for_block(ctx, validator.last_block())
                .await?;
            Ok(())
        })
        .await
        .unwrap();

        tracing::info!("Run p2p fetcher again.");
        scope::run!(ctx, |ctx, s| async {
            let (node, runner) = testonly::StateKeeper::new(ctx, node_store.clone()).await?;
            s.spawn_bg(runner.run(ctx));
            s.spawn_bg(node.run_p2p_fetcher(ctx, client.clone(), node_cfg.clone()));
            validator.push_random_blocks(rng, 3).await;
            let want = validator_store
                .wait_for_certificates_and_verify(ctx, validator.last_block())
                .await?;
            let got = node_store
                .wait_for_certificates_and_verify(ctx, validator.last_block())
                .await?;
            assert_eq!(want, got);
            Ok(())
        })
        .await
        .unwrap();
        Ok(())
    })
    .await
    .unwrap();
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn test_centralized_fetcher(from_snapshot: bool) {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();

    scope::run!(ctx, |ctx, s| async {
        tracing::info!("Spawn a validator.");
        let validator_store = testonly::new_store(from_snapshot).await;
        let (mut validator, runner) =
            testonly::StateKeeper::new(ctx, validator_store.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("validator")));

        tracing::info!("Produce a batch (to make api server start)");
        // TODO: ensure at least L1 batch in `testonly::StateKeeper::new()` to make it fool proof.
        validator.seal_batch().await;

        tracing::info!("Spawn a node.");
        let node_store = testonly::new_store(from_snapshot).await;
        let (node, runner) = testonly::StateKeeper::new(ctx, node_store.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("fetcher")));
        s.spawn_bg(node.run_centralized_fetcher(ctx, validator.connect(ctx).await?));

        tracing::info!("Produce some blocks and wait for node to fetch them");
        validator.push_random_blocks(rng, 10).await;
        let want = validator_store
            .wait_for_block(ctx, validator.last_block())
            .await?;
        let got = node_store
            .wait_for_block(ctx, validator.last_block())
            .await?;
        assert_eq!(want, got);
        Ok(())
    })
    .await
    .unwrap();
}

struct Random<T>(T);

impl<T> RandomConfig for Random<T>
where
    Standard: Distribution<T>,
{
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self(g.rng.gen())
    }
}

impl RandomConfig for Config {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            server_addr: g.gen(),
            public_addr: g.gen(),
            validators: g.rng.gen(),
            max_payload_size: g.gen(),
            gossip_dynamic_inbound_limit: g.gen(),
            gossip_static_inbound: g
                .gen::<Vec<Random<node::SecretKey>>>()
                .into_iter()
                .map(|x| x.0.public())
                .collect(),
            gossip_static_outbound: g
                .gen::<Vec<Random<node::SecretKey>>>()
                .into_iter()
                .map(|x| (x.0.public(), g.gen()))
                .collect(),
        }
    }
}

impl RandomConfig for Secrets {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            validator_key: g.gen::<Option<Random<validator::SecretKey>>>().map(|x| x.0),
            node_key: g.gen::<Option<Random<node::SecretKey>>>().map(|x| x.0),
        }
    }
}

#[test]
fn test_schema_encoding() {
    let ctx = ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    encode_decode::<FmtConv<config::Config>>(rng);
}
