use rand::Rng;
use super::{Add,WeightedValidator,VM, Registry};
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
use zksync_types::{Execute, Transaction};

struct Account(zksync_test_account::Account);

impl Account {
    fn new() -> Self {
        Self(zksync_test_account::random())
    }

    async fn deploy_registry_contract(&mut self, inputs: contracts::Initialize) -> (Registry, Transaction) {
        let tx = self.0.get_deploy_tx(
            &contracts::ConsensusRegistry::bytecode(),
            Some(&inputs.encode()),
            zksync_test_account::TxType::L2,
        );
        (Registry(tx), tx.tx)
    }

    fn make_tx<F:contracts::Function>(&self, call: contracts::Call<F>) -> Transaction {
        self.0.get_l2_tx_for_execute(
            Execute {
                contract_address: call.address,
                calldata: call.calldata().unwrap(),
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

        let vm = VM::new(pool.clone()).await;
        let mut account = Account::new();
        let (registry, tx) = account.deploy_consensus_registry(contracts::Initialize { initial_owner: account.address() });
        
        let mut txs = vec![tx];
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

fn gen_add_inputs(rng: &mut impl Rng) -> Add {
    let k : validator::SecretKey = rng.gen();
    Add {
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
