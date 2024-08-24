use anyhow::Context as _;
use test_casing::{test_casing, Product};
use tracing::Instrument as _;
use zksync_concurrency::{ctx, error::Wrap, scope};
use zksync_config::ContractsConfig;
use zksync_consensus_roles::{
    attester,
    validator::testonly::{Setup, SetupSpec},
};
use zksync_dal::consensus_dal::AttestationStatus;
use zksync_node_sync::MainNodeClient;
use zksync_types::{L1BatchNumber, ProtocolVersionId};

use super::{FROM_SNAPSHOT, VERSIONS};
use crate::{mn::run_main_node, storage::ConnectionPool, testonly};

#[test_casing(2, VERSIONS)]
#[tokio::test]
async fn test_attestation_status_api(version: ProtocolVersionId) {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test(false, version).await;
        let (mut sk, runner) = testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run(ctx).instrument(tracing::info_span!("validator")));

        // Setup nontrivial genesis.
        while sk.last_sealed_batch() < L1BatchNumber(3) {
            sk.push_random_blocks(rng, 10).await;
        }
        let mut setup = SetupSpec::new(rng, 3);
        setup.first_block = sk.last_block();
        let first_batch = sk.last_batch();
        let setup = Setup::from(setup);
        let mut conn = pool.connection(ctx).await.wrap("connection()")?;
        conn.try_update_genesis(ctx, &setup.genesis)
            .await
            .wrap("try_update_genesis()")?;
        // Make sure that the first_batch is actually sealed.
        sk.seal_batch().await;
        pool.wait_for_batch(ctx, first_batch).await?;

        // Connect to API endpoint.
        let api = sk.connect(ctx).await?;
        let fetch_status = || async {
            let s = api
                .fetch_attestation_status()
                .await?
                .context("no attestation_status")?;
            let s: AttestationStatus =
                zksync_protobuf::serde::deserialize(&s.0).context("deserialize()")?;
            anyhow::ensure!(s.genesis == setup.genesis.hash(), "genesis hash mismatch");
            Ok(s)
        };

        // If the main node has no L1 batch certificates,
        // then the first one to sign should be the batch with the `genesis.first_block`.
        let status = fetch_status().await?;
        assert_eq!(
            status.next_batch_to_attest,
            attester::BatchNumber(first_batch.0.into())
        );
        assert_eq!(
            status.consensus_registry_address,
            ContractsConfig::for_tests().l2_consensus_registry_addr
        );

        // Insert a (fake) cert, then check again.
        {
            let mut conn = pool.connection(ctx).await?;
            let number = status.next_batch_to_attest;
            let hash = conn.batch_hash(ctx, number).await?.unwrap();
            let genesis = conn.genesis(ctx).await?.unwrap().hash();
            let cert = attester::BatchQC {
                signatures: attester::MultiSig::default(),
                message: attester::Batch {
                    number,
                    hash,
                    genesis,
                },
            };
            conn.insert_batch_certificate(ctx, &cert)
                .await
                .context("insert_batch_certificate()")?;
        }
        let want = status.next_batch_to_attest.next();
        let got = fetch_status().await?;
        assert_eq!(want, got.next_batch_to_attest);

        Ok(())
    })
    .await
    .unwrap();
}

// Test running a couple of attesters (which are also validators).
// Main node is expected to collect all certificates.
// External nodes are expected to just vote for the batch.
//
// TODO: it would be nice to use `StateKeeperRunner::run_real()` in this test,
// however as of now it doesn't work with ENs and it doesn't work with
// `ConnectionPool::from_snapshot`.
#[test_casing(4, Product((FROM_SNAPSHOT,VERSIONS)))]
#[tokio::test]
async fn test_multiple_attesters(from_snapshot: bool, version: ProtocolVersionId) {
    const NODES: usize = 4;

    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::AffineClock::new(10.));
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 4);
    let cfgs = testonly::new_configs(rng, &setup, NODES);

    scope::run!(ctx, |ctx, s| async {
        let validator_pool = ConnectionPool::test(from_snapshot, version).await;
        let (mut validator, runner) =
            testonly::StateKeeper::new(ctx, validator_pool.clone()).await?;
        s.spawn_bg(async {
            runner
                .run(ctx)
                .instrument(tracing::info_span!("validator"))
                .await
                .context("validator")
        });
        // API server needs at least 1 L1 batch to start.
        validator.seal_batch().await;
        validator_pool
            .wait_for_payload(ctx, validator.last_block())
            .await?;

        tracing::info!("Run validator.");
        s.spawn_bg(run_main_node(
            ctx,
            cfgs[0].config.clone(),
            cfgs[0].secrets.clone(),
            validator_pool.clone(),
        ));

        tracing::info!("Run nodes.");
        let mut node_pools = vec![];
        for (i, cfg) in cfgs[1..].iter().enumerate() {
            let i = ctx::NoCopy(i);
            let pool = ConnectionPool::test(from_snapshot, version).await;
            let (node, runner) = testonly::StateKeeper::new(ctx, pool.clone()).await?;
            node_pools.push(pool.clone());
            s.spawn_bg(async {
                let i = i;
                runner
                    .run(ctx)
                    .instrument(tracing::info_span!("node", i = *i))
                    .await
                    .with_context(|| format!("node{}", *i))
            });
            s.spawn_bg(node.run_consensus(ctx, validator.connect(ctx).await?, cfg.clone()));
        }

        tracing::info!("Create some batches");
        validator.push_random_blocks(rng, 20).await;
        validator.seal_batch().await;
        tracing::info!("Wait for the batches to be attested");
        let want_last = attester::BatchNumber(validator.last_sealed_batch().0.into());
        validator_pool
            .wait_for_batch_certificates_and_verify(ctx, want_last)
            .await?;
        Ok(())
    })
    .await
    .unwrap();
}
