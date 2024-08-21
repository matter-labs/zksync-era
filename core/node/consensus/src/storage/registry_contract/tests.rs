use rand::Rng;
use super::{AddInputs,WeightedValidator,VMReader};
use zksync_basic_types::{web3::contract::Tokenize, L1BatchNumber};
use zksync_concurrency::{ctx, scope};
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::{attester, validator};
use zksync_types::{
    api::{BlockId, BlockNumber},
    ethabi,
    L2ChainId, ProtocolVersionId, U256,
};
use crate::storage::ConnectionPool;
use zksync_basic_types::{ethabi};
use zksync_state_keeper::testonly::fee;
use zksync_test_account::{Account, DeployContractsTx, TxType};
use zksync_types::{Execute, Transaction};

fn make_deploy_tx(account: &mut Account, owner: ethabi::Address) -> DeployContractsTx {
    account.get_deploy_tx(
        &contracts::ConsensusRegistry::bytecode(),
        Some(&owner.into_tokens()),
        TxType::L2,
    )
}

fn make_tx(account: &mut Account, address: ethabi::Address, calldata: Vec<u8>) -> Transaction {
    account.get_l2_tx_for_execute(
        Execute {
            address,
            calldata,
            value: Default::default(),
            factory_deps: vec![],
        },
        Some(fee(10_000_000)),
    )
}

async fn make_tx_sender(pool: ConnectionPool) -> zksync_node_api_server::tx_sender::TxSender {
    zksync_node_api_server::web3::testonly::create_test_tx_sender(
        pool,
        L2ChainId::default(),
        zksync_node_api_server::execution_sandbox::TransactionExecutor::Real,
    )
    .await.0
}

#[tokio::test(flavor = "multi_thread")]
async fn test_vm_reader() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test(false, ProtocolVersionId::latest()).await;
        let (node, runner) = crate::testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run_real(ctx));

        let mut account = Account::random();
        let deploy_tx = make_deploy_tx(&mut account, account.address);
        let vm = VMReader::new(make_tx_sender(&pool).await, deploy_tx.address);
        
        let mut txs = vec![deploy_tx.tx];
        let mut attesters = vec![];
        for _ in 0..5 {
            let input = gen_add_inputs(rng);
            attesters.push(input.attester);
            txs.push(gen_tx(&mut account, vm.add().encode_input(input))); 
        }
        txs.push(gen_tx(&mut account, vm.set_committees().encode_input(()))).await;
        node.push_block(txs).await;
        node.seal_batch().await;
        node.wait_for_batch(node.last_batch()).await;

        let want = attester::Committee::new(attesters.into_iter()).unwrap();
        let conn = &mut pool.connection().await.unwrap();
        assert_eq!(want, vm.get_attester_committee(ctx, conn, node.last_batch()).await.unwrap());
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_committee_extractor() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test(false, ProtocolVersionId::latest()).await;
        let (node, runner) = crate::testonly::StateKeeper::new(ctx, pool.clone()).await?;
        let account = runner.account.clone();
        s.spawn_bg(runner.run_real(ctx));

        let mut writer = VMWriter::new(pool.clone(), node, account.clone(), account.address);
        let tx_sender = make_tx_sender(&pool).await; 
        let reader = VMReader::new(pool.clone(), tx_sender.clone(), writer.deploy_tx.address);
        let extractor = CommitteeExtractor::new(pool.clone(), reader);
        s.spawn_bg(extractor.run(ctx));

        let mut want: Vec<(L1BatchNumber, attester::Committee)> = vec![];
        let batch_number = writer.seal_batch_and_wait(ctx).await;
        want.push((batch_number, attester::Committee::default()));

        writer.deploy().await;
        let batch_number = writer.seal_batch_and_wait(ctx).await;
        want.push((batch_number, attester::Committee::default()));

        let nodes = gen_random_abi_nodes(ctx, 5);
        let committee = attester::Committee::new(nodes.iter().map(abi_node_to_attester)).unwrap();
        writer.add_nodes(&nodes.iter().collect::<Vec<_>>()).await;
        let batch_number = writer.seal_batch_and_wait(ctx).await;
        want.push((batch_number, attester::Committee::default()));

        writer.set_committees().await;
        let batch_number = writer.seal_batch_and_wait(ctx).await;
        want.push((batch_number, committee.clone()));

        writer.remove_nodes(&nodes.iter().collect::<Vec<_>>()).await;
        let batch_number = writer.seal_batch_and_wait(ctx).await;
        want.push((batch_number, committee.clone()));

        writer.set_committees().await;
        let batch_number = writer.seal_batch_and_wait(ctx).await;
        want.push((batch_number, attester::Committee::default()));

        let nodes = gen_random_abi_nodes(ctx, 5);
        let committee = attester::Committee::new(nodes.iter().map(abi_node_to_attester)).unwrap();
        writer.add_nodes(&nodes.iter().collect::<Vec<_>>()).await;
        let batch_number = writer.seal_batch_and_wait(ctx).await;
        want.push((batch_number, attester::Committee::default()));

        writer.set_committees().await;
        let batch_number = writer.seal_batch_and_wait(ctx).await;
        want.push((batch_number, committee.clone()));

        let batch_number = writer.seal_batch_and_wait(ctx).await;
        want.push((batch_number, committee.clone()));

        let batch_number = writer.seal_batch_and_wait(ctx).await;
        want.push((batch_number, committee.clone()));

        // TODO: wait for getting stuff populated.
        for (batch, want) in want {
            let got = pool.connection(ctx).await?.attester_committee(ctx, batch).await?;
            assert_eq!(got, Some(want));
        }
        Ok(())
    })
    .await
    .unwrap();
}

fn gen_add_inputs(rng: &mut impl Rng) -> AddInputs {
    AddInputs {
        node_owner: ethabi::Address::random(),
        // TODO: generate valid pop.
        validator: WeightedValidator {
            key: rng.gen(),
            weight: rng.gen_range(1..100),
            pop: rng.gen(),
        },
        attester: attester::WeightedAttester {
            key: rng.gen(),
            weight: rng.gen_range(1..100),
        },
    }
}
