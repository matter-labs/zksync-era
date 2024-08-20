use rand::Rng;
use super::committee_extractor::CommitteeExtractor;
use super::{testonly::VMWriter, vm_reader::VMReader};
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

async fn make_tx_sender(pool: &ConnectionPool) -> zksync_node_api_server::tx_sender::TxSender {
    zksync_node_api_server::web3::testonly::create_test_tx_sender(
        pool.0.clone(),
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
        let account = runner.account.clone();
        s.spawn_bg(runner.run_real(ctx));

        let mut writer =
            super::testonly::VMWriter::new(pool.clone(), node, account.clone(), account.address);

        let mut want = gen_random_abi_nodes(ctx, 5);
        writer.deploy().await;
        writer.add_nodes(want).await;
        writer.set_committees().await;
        writer.seal_batch_and_wait(ctx).await;

        let tx_sender = make_tx_sender(&pool).await;
        let block_id = BlockId::Number(BlockNumber::Pending);
        let reader = super::vm_reader::VMReader::new(pool.clone(), tx_sender, writer.deploy_tx.address);
        assert_eq!(want, reader.read_attester_committee(ctx, block_id).await.unwrap());
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

fn gen_random_abi_nodes(ctx: &ctx::Ctx, num_nodes: usize) -> Vec<Vec<ethabi::Token>> {
    let rng = &mut ctx.rng();
    (0..num_nodes).map(|_|(
        ethabi::Address::random(),
        U256::from(rng.gen::<usize>()),
        rng.gen::<validator::PublicKey>().encode(),
        rng.gen::<validator::Signature>().encode(),
        U256::from(rng.gen::<usize>()),
        rng.gen::<attester::PublicKey>().encode(),
    ).into_tokens()).collect()
}

fn abi_node_to_attester(node: &Vec<ethabi::Token>) -> attester::WeightedAttester {
    let pub_key = &node[5].clone().into_bytes().unwrap();
    let weight = node[4].clone().into_uint().unwrap();
    attester::WeightedAttester {
        key: attester::PublicKey::decode(pub_key).unwrap(),
        weight: weight.try_into().unwrap(),
    }
}
