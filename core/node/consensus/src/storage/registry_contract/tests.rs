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
use zksync_test_account::Account;

impl Registry {
    fn deploy(account: &mut Account, initial_owner: ethabi::Address) -> (Registry, Transaction) {
        let tx = account.get_deploy_tx(
            &contracts::ConsensusRegistry::bytecode(),
            Some(&inputs.encode()),
            zksync_test_account::TxType::L2,
        );
        (Registry::at(tx.address), tx.tx)
    }

    fn add(&self, node_owner: ethabi::Address, validator: WeightedValidator, attester: attester::WeightedAttester) -> Transaction {
        self.make_tx(self.0.add(
            node_owner,
            encode_validator_key(&validator.key),
            validator.weight.into(),
            encode_validator_pop(&validator.pop),
            encode_attester_key(&attester.key),
            attester.weight.try_into().context("overflow")?,
        )).unwrap()
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
        let mut account = Account::random();
        let (registry, tx) = Registry::deploy(account.address());
        
        let mut committee = attesters::Committee::new((0..5).map(|_|gen_attester(rng))).unwrap();
        let mut txs = vec![tx];
        for _ in 0..5 {
            txs.push((&mut account, vm.add().encode_input(input))); 
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

fn gen_validator(rng: &mut impl Rng) -> WeightedValidator {
    let k : validator::SecretKey = rng.gen();
    WeightedValidator {
        key: k.public(),
        weight: rng.gen_range(1..100),
        pop: k.sign_pop(),
    }
}

fn gen_attester(rng: &mut impl Rng) -> attester::WeightedAttester {
    attester::WeightedAttester {
        key: rng.gen(),
        weight: rng.gen_range(1..100),
    }
}
