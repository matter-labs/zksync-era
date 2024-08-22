use rand::Rng;
use super::{AddInputs,WeightedValidator,VM};
use zksync_basic_types::{web3::contract::Tokenize};
use zksync_concurrency::{ctx, scope};
use zksync_contracts::consensus_l2_contracts as contracts;
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::{attester, validator};
use zksync_types::{
    ethabi,
    L2ChainId, ProtocolVersionId,
};
use crate::storage::ConnectionPool;
use zksync_state_keeper::testonly::fee;
use zksync_test_account::{Account, TxType};
use zksync_types::{Execute, Transaction};

impl VM {
    async fn deploy(pool: ConnectionPool, account: &mut Account, owner: ethabi::Address) -> (Self, Transaction) {
        let deploy_tx = account.get_deploy_tx(
            &contracts::ConsensusRegistry::bytecode(),
            Some(&owner.into_tokens()),
            TxType::L2,
        );
        let tx_sender = zksync_node_api_server::web3::testonly::create_test_tx_sender(
            pool,
            L2ChainId::default(),
            zksync_node_api_server::execution_sandbox::TransactionExecutor::Real,
        )
        .await.0;
        (VM::new(tx_sender,deploy_tx.address),deploy_tx.tx)
    }

    fn add(&self, account: &mut Account, inputs: AddInputs) -> Transaction {
        self.make_tx(account,self.contract.add().encode_inputs(inputs.encode()))
    }

    fn make_tx(&self, account: &mut Account, calldata: Vec<u8>) -> Transaction {
        account.get_l2_tx_for_execute(
            Execute {
                address: self.address,
                calldata,
                value: Default::default(),
                factory_deps: vec![],
            },
            Some(fee(10_000_000)),
        )
    }
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
        let (vm, deploy_tx) = VM::deploy(pool, &mut account, account.address).await;
        
        let mut txs = vec![deploy_tx];
        let mut attesters = vec![];
        for _ in 0..5 {
            let input = gen_add_inputs(rng);
            attesters.push(input.attester);
            txs.push(make_tx(&mut account, vm.add().encode_input(input))); 
        }
        txs.push(make_tx(&mut account, vm.set_committees().encode_input(()))).await;
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

fn gen_add_inputs(rng: &mut impl Rng) -> AddInputs {
    let k : validator::SecretKey = rng.gen();
    AddInputs {
        node_owner: ethabi::Address::random(),
        validator: WeightedValidator {
            key: k.public(),
            weight: rng.gen_range(1..100),
            pop: k.sign_pop(),
        },
        attester: attester::WeightedAttester {
            key: rng.gen(),
            weight: rng.gen_range(1..100),
        },
    }
}
