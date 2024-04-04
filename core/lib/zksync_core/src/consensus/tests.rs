use anyhow::Context as _;
use rand::{distributions::Distribution, Rng};
use test_casing::test_casing;
use tracing::Instrument as _;
use zksync_concurrency::{ctx, scope};
use zksync_consensus_executor as executor;
use zksync_consensus_network as network;
use zksync_consensus_network::testonly::{new_configs, new_fullnode};
use zksync_consensus_roles::validator::testonly::Setup;
use zksync_consensus_storage as storage;
use zksync_consensus_storage::PersistentBlockStore as _;
use zksync_consensus_utils::EncodeDist;
use zksync_protobuf::testonly::{test_encode_all_formats, FmtConv};
use zksync_types::{L1BatchNumber, MiniblockNumber};

use super::*;
use crate::utils::testonly::Snapshot;

async fn new_store(from_snapshot: bool) -> Store {
    match from_snapshot {
        true => {
            Store::from_snapshot(Snapshot::make(L1BatchNumber(23), MiniblockNumber(87), &[])).await
        }
        false => Store::from_genesis().await,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_validator_block_store() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let store = new_store(false).await;

    // Fill storage with unsigned miniblocks.
    // Fetch a suffix of blocks that we will generate (fake) certs for.
    let want = scope::run!(ctx, |ctx, s| async {
        // Start state keeper.
        let (mut sk, runner) = testonly::StateKeeper::new(ctx, store.clone()).await?;
        s.spawn_bg(runner.run(ctx));
        sk.push_random_blocks(rng, 10).await;
        store.wait_for_payload(ctx, sk.last_block()).await?;
        let fork = validator::Fork {
            number: validator::ForkNumber(rng.gen()),
            first_block: validator::BlockNumber(4),
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
        public_addr: cfg.public_addr.clone(),
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
#[test_casing(2, [false, true])]
#[tokio::test(flavor = "multi_thread")]
async fn test_validator(from_snapshot: bool) {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);
    let cfgs = new_configs(rng, &setup, 0);

    scope::run!(ctx, |ctx, s| async {
        tracing::info!("Start state keeper.");
        let store = new_store(from_snapshot).await;
        let (mut sk, runner) = testonly::StateKeeper::new(ctx, store.clone()).await?;
        s.spawn_bg(runner.run(ctx));

        tracing::info!("Populate storage with a bunch of blocks.");
        sk.push_random_blocks(rng, 5).await;
        store
            .wait_for_payload(ctx, sk.last_block())
            .await
            .context("sk.wait_for_miniblocks(<1st phase>)")?;

        tracing::info!("Restart consensus actor a couple times, making it process a bunch of blocks each time.");
        for iteration in 0..3 {
            tracing::info!("iteration {iteration}");
            scope::run!(ctx, |ctx, s| async {
                tracing::info!("Start consensus actor");
                // In the first iteration it will initialize genesis.
                let cfg = MainNodeConfig {
                    executor: executor_config(&cfgs[0]),
                    validator_key: setup.keys[0].clone(),
                };
                s.spawn_bg(cfg.run(ctx, store.clone()));

                tracing::info!("Generate couple more blocks and wait for consensus to catch up.");
                sk.push_random_blocks(rng, 3).await;
                store
                    .wait_for_certificate(ctx, sk.last_block())
                    .await
                    .context("wait_for_certificate(<2nd phase>)")?;

                tracing::info!("Synchronously produce blocks one by one, and wait for consensus.");
                for _ in 0..2 {
                    sk.push_random_blocks(rng, 1).await;
                    store
                        .wait_for_certificate(ctx, sk.last_block())
                        .await
                        .context("wait_for_certificate(<3rd phase>)")?;
                }

                tracing::info!("Verify all certificates");
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

// Test running a validator node and 2 full nodes recovered from different snapshots.
#[tokio::test(flavor = "multi_thread")]
async fn test_nodes_from_various_snapshots() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);
    let validator_cfg = new_configs(rng, &setup, 0).pop().unwrap();

    scope::run!(ctx, |ctx, s| async {
        tracing::info!("spawn validator");
        let validator_store = Store::from_genesis().await;
        let (mut validator, runner) =
            testonly::StateKeeper::new(ctx, validator_store.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("validator")));
        let cfg = MainNodeConfig {
            executor: executor_config(&validator_cfg),
            validator_key: setup.keys[0].clone(),
        };
        s.spawn_bg(cfg.run(ctx, validator_store.clone()));

        tracing::info!("produce some batches");
        validator.push_random_blocks(rng, 5).await;
        validator.seal_batch().await;
        validator_store
            .wait_for_certificate(ctx, validator.last_block())
            .await?;

        tracing::info!("take snapshot and start a node from it");
        let snapshot = validator_store.snapshot(ctx).await?;
        let node_store = Store::from_snapshot(snapshot).await;
        let (node, runner) = testonly::StateKeeper::new(ctx, node_store.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("node1")));
        let node_cfg = executor_config(&new_fullnode(rng, &validator_cfg));
        s.spawn_bg(node.run_p2p_fetcher(ctx, validator.connect(ctx).await?, node_cfg));

        tracing::info!("produce more batches");
        validator.push_random_blocks(rng, 5).await;
        validator.seal_batch().await;
        node_store
            .wait_for_certificate(ctx, validator.last_block())
            .await?;

        tracing::info!("take another snapshot and start a node from it");
        let snapshot = validator_store.snapshot(ctx).await?;
        let node_store2 = Store::from_snapshot(snapshot).await;
        let (node, runner) = testonly::StateKeeper::new(ctx, node_store2.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("node2")));
        let node_cfg = executor_config(&new_fullnode(rng, &validator_cfg));
        s.spawn_bg(node.run_p2p_fetcher(ctx, validator.connect(ctx).await?, node_cfg));

        tracing::info!("produce more blocks and compare storages");
        validator.push_random_blocks(rng, 5).await;
        let want = validator_store
            .wait_for_certificates_and_verify(ctx, validator.last_block())
            .await?;
        // node stores should be suffixes for validator store.
        for got in [
            node_store
                .wait_for_certificates_and_verify(ctx, validator.last_block())
                .await?,
            node_store2
                .wait_for_certificates_and_verify(ctx, validator.last_block())
                .await?,
        ] {
            assert_eq!(want[want.len() - got.len()..], got[..]);
        }
        Ok(())
    })
    .await
    .unwrap();
}

// Test running a validator node and a couple of full nodes.
// Validator is producing signed blocks and fetchers are expected to fetch
// them directly or indirectly.
#[test_casing(2, [false, true])]
#[tokio::test(flavor = "multi_thread")]
async fn test_full_nodes(from_snapshot: bool) {
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
        let validator_store = new_store(from_snapshot).await;
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
            .wait_for_payload(ctx, validator.last_block())
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
            let store = new_store(from_snapshot).await;
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
#[test_casing(2, [false, true])]
#[tokio::test(flavor = "multi_thread")]
async fn test_p2p_fetcher_backfill_certs(from_snapshot: bool) {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);
    let validator_cfg = new_configs(rng, &setup, 0)[0].clone();
    let node_cfg = executor_config(&new_fullnode(rng, &validator_cfg));

    scope::run!(ctx, |ctx, s| async {
        tracing::info!("Spawn validator.");
        let validator_store = new_store(from_snapshot).await;
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
        // API server needs at least 1 L1 batch to start.
        validator.seal_batch().await;
        let client = validator.connect(ctx).await?;

        let node_store = new_store(from_snapshot).await;

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
                .wait_for_payload(ctx, validator.last_block())
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
        let validator_store = new_store(from_snapshot).await;
        let (mut validator, runner) =
            testonly::StateKeeper::new(ctx, validator_store.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("validator")));

        tracing::info!("Produce a batch (to make api server start)");
        // TODO: ensure at least L1 batch in `testonly::StateKeeper::new()` to make it fool proof.
        validator.seal_batch().await;

        tracing::info!("Spawn a node.");
        let node_store = new_store(from_snapshot).await;
        let (node, runner) = testonly::StateKeeper::new(ctx, node_store.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("fetcher")));
        s.spawn_bg(node.run_centralized_fetcher(ctx, validator.connect(ctx).await?));

        tracing::info!("Produce some blocks and wait for node to fetch them");
        validator.push_random_blocks(rng, 10).await;
        let want = validator_store
            .wait_for_payload(ctx, validator.last_block())
            .await?;
        let got = node_store
            .wait_for_payload(ctx, validator.last_block())
            .await?;
        assert_eq!(want, got);
        Ok(())
    })
    .await
    .unwrap();
}

impl Distribution<Config> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Config {
        Config {
            server_addr: self.sample(rng),
            public_addr: self.sample(rng),
            validators: rng.gen(),
            max_payload_size: self.sample(rng),
            gossip_dynamic_inbound_limit: self.sample(rng),
            gossip_static_inbound: self.sample_range(rng).map(|_| rng.gen()).collect(),
            gossip_static_outbound: self
                .sample_range(rng)
                .map(|_| (rng.gen(), self.sample(rng)))
                .collect(),
        }
    }
}

impl Distribution<Secrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Secrets {
        Secrets {
            validator_key: self.sample_opt(|| rng.gen()),
            node_key: self.sample_opt(|| rng.gen()),
        }
    }
}

#[test]
fn test_schema_encoding() {
    let ctx = ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    test_encode_all_formats::<FmtConv<config::Config>>(rng);
}
