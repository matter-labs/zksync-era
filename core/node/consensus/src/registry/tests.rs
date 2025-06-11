use rand::Rng as _;
use zksync_concurrency::{ctx, scope};
use zksync_consensus_roles::validator;
use zksync_test_contracts::Account;
use zksync_types::ProtocolVersionId;

use super::*;
use crate::storage::ConnectionPool;

/// Test checking that parsing logic matches the abi specified in the json file.
#[ignore = "We still use the old abi. When the new consensus registry is merged, this test should be enabled."]
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

#[ignore = "We still use the old abi. When the new consensus registry is merged, this test should be enabled."]
#[tokio::test(flavor = "multi_thread")]
async fn test_current_validator_committee() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let account = &mut Account::random();
    let to_fund = &[account.address];

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test(false, ProtocolVersionId::latest()).await;

        let (mut node, runner) = crate::testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run_real(ctx, to_fund));

        // Deploy registry contract and initialize it.
        let (registry_addr, tx) = Registry::deploy(account);
        let registry = Registry::new(pool.clone(), registry_addr).await;
        let mut txs = vec![tx];
        let account_addr = account.address();
        txs.push(testonly::make_tx(
            account,
            registry_addr,
            registry.initialize(account_addr),
        ));

        // Generate validators.
        let validators: Vec<_> = (0..5).map(|_| testonly::gen_validator(rng)).collect();
        // This is the default leader selection for the Registry contract.
        let leader_selection = validator::LeaderSelection {
            frequency: 1,
            mode: validator::LeaderSelectionMode::RoundRobin,
        };
        let validator_infos: Vec<_> = validators
            .iter()
            .map(|v| validator::ValidatorInfo {
                key: v.key.clone(),
                weight: v.weight,
                leader: v.is_leader,
            })
            .collect();
        let schedule = validator::Schedule::new(validator_infos, leader_selection).unwrap();

        // Add validators to the registry.
        for v in validators.iter() {
            txs.push(testonly::make_tx(
                account,
                registry_addr,
                registry.add(rng.gen(), v.clone()).unwrap(),
            ));
        }

        // Commit the update.
        txs.push(testonly::make_tx(
            account,
            registry_addr,
            registry.commit_validator_committee(),
        ));

        // Push the block and wait for it to be executed.
        node.push_block(&txs).await;
        let block_num = node.last_block();
        pool.wait_for_payload(ctx, block_num)
            .await
            .wrap("wait_for_payload()")?;

        // Read the validator schedule using the vm.
        let (actual_schedule, commit_block) = registry
            .get_current_validator_schedule(ctx, block_num)
            .await
            .unwrap();

        // Check the schedule and commit block number.
        assert_eq!(schedule, actual_schedule);
        assert_eq!(block_num, commit_block);

        Ok(())
    })
    .await
    .unwrap();
}

#[ignore = "We still use the old abi. When the new consensus registry is merged, this test should be enabled."]
#[tokio::test(flavor = "multi_thread")]
async fn test_pending_validator_committee() {
    const DELAY: u32 = 100;

    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let account = &mut Account::random();
    let to_fund = &[account.address];

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test(false, ProtocolVersionId::latest()).await;

        let (mut node, runner) = crate::testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run_real(ctx, to_fund));

        // Deploy registry contract and initialize it.
        let (registry_addr, tx) = Registry::deploy(account);
        let registry = Registry::new(pool.clone(), registry_addr).await;
        let mut txs = vec![tx];
        let account_addr = account.address();
        txs.push(testonly::make_tx(
            account,
            registry_addr,
            registry.initialize(account_addr),
        ));

        // Generate validators.
        let validators: Vec<_> = (0..5).map(|_| testonly::gen_validator(rng)).collect();
        // This is the default leader selection for the Registry contract.
        let leader_selection = validator::LeaderSelection {
            frequency: 1,
            mode: validator::LeaderSelectionMode::RoundRobin,
        };
        let validator_infos: Vec<_> = validators
            .iter()
            .map(|v| validator::ValidatorInfo {
                key: v.key.clone(),
                weight: v.weight,
                leader: v.is_leader,
            })
            .collect();
        let schedule = validator::Schedule::new(validator_infos, leader_selection).unwrap();

        // *Change the activation delay so it's not immediate.*
        txs.push(testonly::make_tx(
            account,
            registry_addr,
            registry.set_committee_activation_delay(DELAY),
        ));

        // Add validators to the registry.
        for v in validators.iter() {
            txs.push(testonly::make_tx(
                account,
                registry_addr,
                registry.add(rng.gen(), v.clone()).unwrap(),
            ));
        }

        // Commit the update.
        txs.push(testonly::make_tx(
            account,
            registry_addr,
            registry.commit_validator_committee(),
        ));

        // Push the block and wait for it to be executed.
        node.push_block(&txs).await;
        let block_num = node.last_block();
        pool.wait_for_payload(ctx, block_num)
            .await
            .wrap("wait_for_payload()")?;

        // Read the *pending* validator schedule using the vm.
        let (actual_schedule, commit_block) = registry
            .get_pending_validator_schedule(ctx, block_num)
            .await
            .unwrap()
            .unwrap();

        // Check the schedule and commit block number.
        assert_eq!(schedule, actual_schedule);
        assert_eq!(block_num + DELAY as u64, commit_block);

        Ok(())
    })
    .await
    .unwrap();
}
