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
use zksync_dal::{ConnectionPool};
use zksync_protobuf::testonly::test_encode_random;

use super::*;

#[test]
fn test_schema_encoding() {
    let ctx = ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    test_encode_random::<config::Config>(rng);
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
        sk.sync_state.wait_for_local_block(ctx,sk.last_miniblock()).await?;
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
        sk.sync_state.wait_for_local_block(ctx, sk.last_miniblock())
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
            node_cfgs.last().unwrap_or(&validator_cfgs[0]),
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
        validator.sync_state.wait_for_local_block(ctx, validator.last_miniblock()).await.unwrap();

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
                store: node.store,
                client: validator.connect(ctx).await?,
                sync_state: node.sync_state,
            };
            s.spawn_bg(fetcher.run_p2p(ctx, node.actions_sender, executor_config(cfg)));
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
async fn test_p2p_fetcher_backfill_certs() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);
    let validator_cfg = new_configs(rng, &setup, 0)[0].clone();
    let node_cfg = new_fullnode(rng, &validator_cfg);

    scope::run!(ctx, |ctx, s| async {
        tracing::info!("Spawn validator.");
        let pool = ConnectionPool::test_pool().await;
        let (mut validator, runner) = testonly::StateKeeper::new(pool).await?;
        s.spawn_bg(runner.run(ctx));
        s.spawn_bg(MainNodeConfig {
            executor: executor_config(&validator_cfg),
            validator_key: setup.keys[0].clone(),
        }.run(ctx, validator.store.clone()));

    
        let pool = ConnectionPool::test_pool().await;
        
        tracing::info!("Run p2p fetcher.");
        scope::run!(ctx, |ctx, s| async {
            let (node, runner) = testonly::StateKeeper::new(pool.clone()).await?;
            s.spawn_bg(runner.run(ctx));
            s.spawn_bg(Fetcher {
                store: node.store.clone(),
                client: validator.connect(ctx).await?,
                sync_state: node.sync_state,
            }.run_p2p(ctx, node.actions_sender, executor_config(&node_cfg)));
            validator.push_random_blocks(rng, 3).await;
            node.store.wait_for_certificate(ctx, validator.last_block()).await?;
            Ok(())
        }).await.unwrap();

        tracing::info!("Run centralized fetcher.");
        scope::run!(ctx, |ctx, s| async {
            let (node, runner) = testonly::StateKeeper::new(pool.clone()).await?;
            s.spawn_bg(runner.run(ctx));
            s.spawn_bg(Fetcher {
                store: node.store.clone(),
                client: validator.connect(ctx).await?,
                sync_state: node.sync_state.clone(),
            }.run_centralized(ctx, node.actions_sender));
            validator.push_random_blocks(rng, 3).await;
            node.sync_state.wait_for_local_block(ctx, validator.last_miniblock()).await?;
            Ok(())
        }).await.unwrap();

        tracing::info!("Run p2p fetcher again.");
        scope::run!(ctx, |ctx, s| async {
            let (node, runner) = testonly::StateKeeper::new(pool.clone()).await?;
            s.spawn_bg(runner.run(ctx));
            s.spawn_bg(Fetcher {
                store: node.store.clone(),
                client: validator.connect(ctx).await?,
                sync_state: node.sync_state,
            }.run_p2p(ctx, node.actions_sender, executor_config(&node_cfg)));
            validator.push_random_blocks(rng, 3).await;
            let want = validator.store.wait_for_blocks_and_verify(ctx, validator.last_block()).await?;
            let got = node.store.wait_for_blocks_and_verify(ctx,validator.last_block()).await?;
            assert_eq!(want, got);
            Ok(())
        }).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_centralized_fetcher() {
    // spawn validator
    // spawn node (from snapshot)
    // compare blocks
}

/*
#[tokio::test]
async fn fetcher_basics() {
    abort_on_panic();
    set_timeout(TEST_TIMEOUT);
    let ctx = &ctx::test_root(&ctx::RealClock);

    scope::run!(ctx,|ctx,s| async {
        let pool = ConnectionPool::test_pool().await;
        ensure_genesis(&mut storage).await;
        let mut mock_client = MockMainNodeClient::default();
        mock_client.push_l1_batch(0);
        // ^ The genesis L1 batch will not be queried, so we're OK with filling it with non-authentic data
        let mut tx_hashes = VecDeque::from(mock_client.push_l1_batch(1));
        tx_hashes.extend(mock_client.push_l1_batch(2));

        let (actions_sender, mut actions) = ActionQueue::new();
        let sync_state = SyncState::default();
        let fetcher = RpcFetcher {
            sync_state: sync_state.clone(),
            client: Box::new(mock_client),
        };
        s.spawn_bg(fetcher.run(ctx,Store(pool.clone()),actions_sender));

        // Check that `sync_state` is updated.
        sync::wait_for(ctx, &mut sync_state.subscribe(), |s|s.main_node_block()>MiniblockNumber(5)).await.unwrap();

        // Check generated actions. Some basic checks are performed by `ActionQueueSender`.
        let mut current_l1_batch_number = L1BatchNumber(0);
        let mut current_miniblock_number = MiniblockNumber(0);
        let mut tx_count_in_miniblock = 0;
        loop {
            match actions.recv_action() {
                SyncAction::OpenBatch { number, .. } => {
                    current_l1_batch_number += 1;
                    current_miniblock_number += 1; // First miniblock is implicitly opened
                    tx_count_in_miniblock = 0;
                    assert_eq!(number, current_l1_batch_number);
                }
                SyncAction::Miniblock { number, .. } => {
                    current_miniblock_number += 1;
                    tx_count_in_miniblock = 0;
                    assert_eq!(number, current_miniblock_number);
                }
                SyncAction::SealBatch { virtual_blocks, .. } => {
                    assert_eq!(virtual_blocks, 0);
                    assert_eq!(tx_count_in_miniblock, 0);
                    if current_miniblock_number == MiniblockNumber(5) {
                        break;
                    }
                }
                SyncAction::Tx(tx) => {
                    assert_eq!(tx.hash(), tx_hashes.pop_front().unwrap());
                    tx_count_in_miniblock += 1;
                }
                SyncAction::SealMiniblock => {
                    assert_eq!(tx_count_in_miniblock, 1);
                }
            }
        }
    }).await.unwrap();
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn fetcher_with_real_server(snapshot_recovery: bool) {
    let pool = ConnectionPool::test_pool().await;
    // Fill in transactions grouped in multiple L1 batches in the storage. We need at least one L1 batch,
    // so that the API server doesn't hang up waiting for it.
    let (snapshot, tx_hashes) =
        run_state_keeper_with_multiple_l1_batches(pool.clone(), snapshot_recovery).await;
    let mut tx_hashes: VecDeque<_> = tx_hashes.into_iter().flatten().collect();

    

    // Start the fetcher connected to the API server.
    let sync_state = SyncState::default();
    let (actions_sender, mut actions) = ActionQueue::new();
    let client = <dyn MainNodeClient>::json_rpc(&format!("http://{server_addr}/")).unwrap();
    let fetcher = MainNodeFetcher {
        client: CachingMainNodeClient::new(Box::new(client)),
        cursor: IoCursor {
            next_miniblock: snapshot.miniblock_number + 1,
            prev_miniblock_hash: snapshot.miniblock_hash,
            prev_miniblock_timestamp: snapshot.miniblock_timestamp,
            l1_batch: snapshot.l1_batch_number,
        },
        actions: actions_sender,
        sync_state: sync_state.clone(),
        stop_receiver,
    };
    let fetcher_task = tokio::spawn(fetcher.run());

    // Check generated actions.
    let mut current_miniblock_number = snapshot.miniblock_number;
    let mut current_l1_batch_number = snapshot.l1_batch_number + 1;
    let mut tx_count_in_miniblock = 0;
    let miniblock_number_to_tx_count = HashMap::from([
        (snapshot.miniblock_number + 1, 1),
        (snapshot.miniblock_number + 2, 0),
        (snapshot.miniblock_number + 3, 1),
    ]);
    let started_at = Instant::now();
    let deadline = started_at + TEST_TIMEOUT;
    loop {
        let action = tokio::time::timeout_at(deadline.into(), actions.recv_action())
            .await
            .unwrap();
        match action {
            SyncAction::OpenBatch {
                number,
                first_miniblock_info,
                ..
            } => {
                assert_eq!(number, current_l1_batch_number);
                current_miniblock_number += 1; // First miniblock is implicitly opened
                tx_count_in_miniblock = 0;
                assert_eq!(first_miniblock_info.0, current_miniblock_number);
            }
            SyncAction::SealBatch { .. } => {
                current_l1_batch_number += 1;
            }
            SyncAction::Miniblock { number, .. } => {
                current_miniblock_number += 1;
                tx_count_in_miniblock = 0;
                assert_eq!(number, current_miniblock_number);
            }
            SyncAction::Tx(tx) => {
                assert_eq!(tx.hash(), tx_hashes.pop_front().unwrap());
                tx_count_in_miniblock += 1;
            }
            SyncAction::SealMiniblock => {
                assert_eq!(
                    tx_count_in_miniblock,
                    miniblock_number_to_tx_count[&current_miniblock_number]
                );
                if current_miniblock_number == snapshot.miniblock_number + 3 {
                    break;
                }
            }
        }
    }
}*/
