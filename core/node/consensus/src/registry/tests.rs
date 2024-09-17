use rand::Rng as _;
use zksync_concurrency::{ctx, scope};
use zksync_consensus_roles::{attester, validator::testonly::Setup};
use zksync_test_account::Account;
use zksync_types::ProtocolVersionId;

use super::*;
use crate::storage::ConnectionPool;

/// Test checking that parsing logic matches the abi specified in the json file.
#[test]
fn test_consensus_registry_abi() {
    zksync_concurrency::testonly::abort_on_panic();
    let c = abi::ConsensusRegistry::load();
    c.call(abi::GetAttesterCommittee).test().unwrap();
    c.call(abi::Add::default()).test().unwrap();
    c.call(abi::Initialize::default()).test().unwrap();
    c.call(abi::CommitAttesterCommittee).test().unwrap();
    c.call(abi::Owner).test().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_attester_committee() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 10);
    let account = &mut Account::random();
    let to_fund = &[account.address];

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test(false, ProtocolVersionId::latest()).await;
        let registry = Registry::new(setup.genesis.clone(), pool.clone()).await;

        // If the registry contract address is not specified,
        // then the committee from genesis should be returned.
        let got = registry
            .attester_committee_for(ctx, None, attester::BatchNumber(10))
            .await
            .unwrap();
        assert_eq!(setup.genesis.attesters, got);

        let (mut node, runner) = crate::testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run_real(ctx, to_fund));

        // Deploy registry contract and initialize it.
        let committee =
            attester::Committee::new((0..5).map(|_| testonly::gen_attester(rng))).unwrap();
        let (registry_addr, tx) = registry.deploy(account);
        let mut txs = vec![tx];
        let account_addr = account.address();
        txs.push(testonly::make_tx(
            account,
            registry_addr,
            registry.initialize(account_addr),
        ));
        // Add attesters.
        for a in committee.iter() {
            txs.push(testonly::make_tx(
                account,
                registry_addr,
                registry
                    .add(rng.gen(), testonly::gen_validator(rng), a.clone())
                    .unwrap(),
            ));
        }
        // Commit the update.
        txs.push(testonly::make_tx(
            account,
            registry_addr,
            registry.commit_attester_committee(),
        ));

        node.push_block(&txs).await;
        node.seal_batch().await;
        pool.wait_for_batch(ctx, node.last_batch()).await?;

        // Read the attester committee using the vm.
        let batch = attester::BatchNumber(node.last_batch().0.into());
        assert_eq!(
            Some(committee),
            registry
                .attester_committee_for(ctx, Some(registry_addr), batch + 1)
                .await
                .unwrap()
        );
        Ok(())
    })
    .await
    .unwrap();
}
