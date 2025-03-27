use rand::Rng as _;
use zksync_concurrency::{ctx, scope, time};
use zksync_consensus_roles::validator;
use zksync_test_contracts::Account;
use zksync_types::ProtocolVersionId;

use super::*;
use crate::storage::ConnectionPool;

const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(500);

/// Test checking that parsing logic matches the abi specified in the json file.
#[test]
fn test_consensus_registry_abi() {
    zksync_concurrency::testonly::abort_on_panic();
    let c = abi::ConsensusRegistry::load();
    c.call(abi::Add::default()).test().unwrap();
    c.call(abi::Initialize::default()).test().unwrap();
    c.call(abi::CommitValidatorCommittee).test().unwrap();
    c.call(abi::GetValidatorCommittee).test().unwrap();
    c.call(abi::GetNextValidatorCommittee).test().unwrap();
    c.call(abi::SetCommitteeActivationDelay::default())
        .test()
        .unwrap();
    c.call(abi::ValidatorsCommitBlock).test().unwrap();
    c.call(abi::Owner).test().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_pending_validator_committee() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let account = &mut Account::random();
    let to_fund = &[account.address];

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test(false, ProtocolVersionId::latest()).await;
        let registry = Registry::new(pool.clone()).await;

        // If the registry contract address is not specified,
        // then an empty committee should be returned.
        let got = registry
            .get_pending_validator_committee(ctx, None, validator::BlockNumber(10))
            .await
            .unwrap();
        assert!(got.is_none());

        let (mut node, runner) = crate::testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run_real(ctx, to_fund));

        // Deploy registry contract and initialize it.
        let committee =
            validator::Committee::new((0..5).map(|_| testonly::gen_validator(rng))).unwrap();
        let (registry_addr, tx) = registry.deploy(account);
        let mut txs = vec![tx];
        let account_addr = account.address();
        txs.push(testonly::make_tx(
            account,
            registry_addr,
            registry.initialize(account_addr),
        ));

        // Add validators.
        for v in committee.iter() {
            txs.push(testonly::make_tx(
                account,
                registry_addr,
                registry
                    .add(rng.gen(), testonly::gen_validator(rng))
                    .unwrap(),
            ));
        }

        // Commit the update.
        txs.push(testonly::make_tx(
            account,
            registry_addr,
            registry.commit_validator_committee(),
        ));

        node.push_block(&txs).await;
        node.seal_batch().await;
        pool.wait_for_batch_info(ctx, node.last_batch(), POLL_INTERVAL)
            .await
            .wrap("wait_for_batch_info()")?;

        // Read the validator committee using the vm.
        let block_num = node.last_block();

        // Check the committee and commit block number
        let (actual_committee, commit_block) = registry
            .get_pending_validator_committee(ctx, Some(registry_addr), block_num)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(committee, actual_committee);
        assert_eq!(block_num, commit_block);

        Ok(())
    })
    .await
    .unwrap();
}
