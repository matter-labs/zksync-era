use rand::Rng;
use zksync_basic_types::web3::contract::Tokenize;
use zksync_concurrency::{ctx, scope, time};
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::{attester, attester::BatchNumber, validator};
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
        writer.deploy(ctx).await;
        writer.add_nodes(ctx, nodes_slice).await;
        writer.set_committees(ctx).await;

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
async fn test_denormalizer() {
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
        let denormalizer = super::denormalizer::Denormalizer::new(pool.clone(), reader);
        s.spawn_bg(denormalizer.run(ctx));

        let mut nodes: Vec<Vec<Token>> = Vec::new();
        let num_nodes = 5;
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
        let nodes_ref: Vec<&[Token]> = nodes.iter().map(|v| v.as_slice()).collect();
        let nodes_slice: &[&[Token]] = nodes_ref.as_slice();
        writer.deploy(ctx).await;
        writer.add_nodes(ctx, nodes_slice).await;
        writer.set_committees(ctx).await;

        ctx.sleep(time::Duration::seconds(5)).await?;

        let batch_committees = pool
            .connection(ctx)
            .await?
            .batch_committees(ctx, BatchNumber(3))
            .await?;
        panic!("{:?}", batch_committees);

        Ok(())
    })
    .await
    .unwrap();
}
