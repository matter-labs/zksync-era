use rand::Rng;
use zksync_basic_types::{web3::contract::Tokenize, L1BatchNumber};
use zksync_concurrency::{ctx, scope, time};
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::{attester, attester::BatchNumber, validator};
use zksync_dal::consensus::{Attester, AttesterCommittee};
use zksync_types::{
    api::{BlockId, BlockNumber},
    ethabi::{Address, Token},
    L2ChainId, ProtocolVersionId, U256,
};

use crate::storage::ConnectionPool;

#[tokio::test(flavor = "multi_thread")]
async fn test_vm_reader() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test(false, ProtocolVersionId::latest()).await;
        let (node, runner) = crate::testonly::StateKeeper::new(ctx, pool.clone()).await?;
        let account = runner.account.clone();
        s.spawn_bg(runner.run_real(ctx));

        let mut writer =
            super::testonly::VMWriter::new(pool.clone(), node, account.clone(), account.address);

        let mut nodes: Vec<Vec<Token>> = Vec::new();
        let num_nodes = 5;
        for _ in 0..num_nodes {
            let node_entry = (
                Address::random(),
                U256::from(rng.gen::<usize>()),
                (0..256).map(|_| rng.gen()).collect::<Vec<u8>>(),
                (0..256).map(|_| rng.gen()).collect::<Vec<u8>>(),
                U256::from(rng.gen::<usize>()),
                (0..256).map(|_| rng.gen()).collect::<Vec<u8>>(),
            )
                .into_tokens();
            nodes.push(node_entry);
        }
        let nodes_ref: Vec<&[Token]> = nodes.iter().map(|v| v.as_slice()).collect();
        let nodes_slice: &[&[Token]] = nodes_ref.as_slice();
        writer.deploy().await;
        writer.add_nodes(nodes_slice).await;
        writer.set_committees().await;
        writer.seal_batch_and_wait(ctx).await;

        let (tx_sender, _) = zksync_node_api_server::web3::testonly::create_test_tx_sender(
            pool.0.clone(),
            L2ChainId::default(),
            zksync_node_api_server::execution_sandbox::TransactionExecutor::Real,
        )
        .await;
        let block_id = BlockId::Number(BlockNumber::Pending);
        let reader = super::vm_reader::VMReader::new(
            pool.clone(),
            tx_sender.clone(),
            writer.deploy_tx.address,
        );

        let (validators, attesters) = reader.read_committees(ctx, block_id).await.unwrap();
        assert_eq!(validators.len(), num_nodes);
        assert_eq!(attesters.len(), num_nodes);
        for i in 0..nodes.len() {
            assert_eq!(
                nodes[i][0].clone().into_address().unwrap(),
                validators[i].node_owner
            );
            assert_eq!(
                nodes[i][1].clone().into_uint().unwrap().as_usize(),
                validators[i].weight
            );
            assert_eq!(
                nodes[i][2].clone().into_bytes().unwrap(),
                validators[i].pub_key
            );
            assert_eq!(nodes[i][3].clone().into_bytes().unwrap(), validators[i].pop);

            assert_eq!(
                nodes[i][0].clone().into_address().unwrap(),
                attesters[i].node_owner
            );
            assert_eq!(
                nodes[i][4].clone().into_uint().unwrap().as_usize(),
                attesters[i].weight
            );
            assert_eq!(
                nodes[i][5].clone().into_bytes().unwrap(),
                attesters[i].pub_key
            );
        }

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

        let mut writer =
            super::testonly::VMWriter::new(pool.clone(), node, account.clone(), account.address);
        let (tx_sender, _) = zksync_node_api_server::web3::testonly::create_test_tx_sender(
            pool.0.clone(),
            L2ChainId::default(),
            zksync_node_api_server::execution_sandbox::TransactionExecutor::Real,
        )
        .await;
        let reader = super::vm_reader::VMReader::new(
            pool.clone(),
            tx_sender.clone(),
            writer.deploy_tx.address,
        );
        let extractor = super::committee_extractor::CommitteeExtractor::new(pool.clone(), reader);
        s.spawn_bg(extractor.run(ctx));

        let mut expected_batch_committee: Vec<(L1BatchNumber, AttesterCommittee)> = vec![];
        let mut batch_number;

        // batch_number = writer.seal_batch_and_wait(ctx).await;
        // expected_batch_committee.push((batch_number, AttesterCommittee::default()));
        //
        writer.deploy().await;
        batch_number = writer.seal_batch_and_wait(ctx).await;
        expected_batch_committee.push((batch_number, AttesterCommittee::default()));

        let nodes = gen_random_abi_nodes(ctx, 5);
        let nodes_ref: Vec<&[Token]> = nodes.iter().map(|v| v.as_slice()).collect();
        let nodes_slice: &[&[Token]] = nodes_ref.as_slice();
        let attester_committee = AttesterCommittee {
            members: nodes.iter().map(abi_node_to_attester).collect(),
        };
        writer.add_nodes(nodes_slice).await;
        batch_number = writer.seal_batch_and_wait(ctx).await;
        expected_batch_committee.push((batch_number, AttesterCommittee::default()));

        writer.set_committees().await;
        batch_number = writer.seal_batch_and_wait(ctx).await;
        expected_batch_committee.push((batch_number, attester_committee.clone()));

        writer.remove_nodes(nodes_slice).await;
        batch_number = writer.seal_batch_and_wait(ctx).await;
        expected_batch_committee.push((batch_number, attester_committee.clone()));

        writer.set_committees().await;
        batch_number = writer.seal_batch_and_wait(ctx).await;
        expected_batch_committee.push((batch_number, AttesterCommittee::default()));

        let nodes = gen_random_abi_nodes(ctx, 5);
        let nodes_ref: Vec<&[Token]> = nodes.iter().map(|v| v.as_slice()).collect();
        let nodes_slice: &[&[Token]] = nodes_ref.as_slice();
        let attester_committee = AttesterCommittee {
            members: nodes.iter().map(abi_node_to_attester).collect(),
        };
        writer.add_nodes(nodes_slice).await;
        batch_number = writer.seal_batch_and_wait(ctx).await;
        expected_batch_committee.push((batch_number, AttesterCommittee::default()));

        writer.set_committees().await;
        batch_number = writer.seal_batch_and_wait(ctx).await;
        expected_batch_committee.push((batch_number, attester_committee.clone()));

        batch_number = writer.seal_batch_and_wait(ctx).await;
        expected_batch_committee.push((batch_number, attester_committee.clone()));

        batch_number = writer.seal_batch_and_wait(ctx).await;
        expected_batch_committee.push((batch_number, attester_committee.clone()));

        // Allow batch sealing async processing.
        ctx.sleep(time::Duration::seconds(1)).await?;

        for (batch, committee) in expected_batch_committee {
            let batch_committees = pool
                .connection(ctx)
                .await?
                .batch_committee(ctx, BatchNumber(batch.0 as u64))
                .await?;

            assert_eq!(batch_committees, Some(committee));
        }

        Ok(())
    })
    .await
    .unwrap();
}

fn gen_random_abi_nodes(ctx: &ctx::Ctx, num_nodes: usize) -> Vec<Vec<Token>> {
    let rng = &mut ctx.rng();
    let mut nodes: Vec<Vec<Token>> = Vec::new();
    for _ in 0..num_nodes {
        let node_entry = (
            Address::random(),
            U256::from(rng.gen::<usize>()),
            rng.gen::<validator::PublicKey>().encode(),
            rng.gen::<validator::Signature>().encode(),
            U256::from(rng.gen::<usize>()),
            rng.gen::<attester::PublicKey>().encode(),
        )
            .into_tokens();
        nodes.push(node_entry);
    }
    nodes
}

fn abi_node_to_attester(node: &Vec<Token>) -> Attester {
    let pub_key = &node[5].clone().into_bytes().unwrap();
    let weight = node[4].clone().into_uint().unwrap();
    Attester {
        pub_key: attester::PublicKey::decode(pub_key).unwrap(),
        weight: weight.as_u64(),
    }
}
