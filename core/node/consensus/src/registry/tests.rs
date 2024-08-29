use rand::Rng;
use zksync_concurrency::{ctx, scope};
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::{attester, validator, validator::testonly::Setup};
use zksync_contracts::consensus as contracts;
use zksync_test_account::Account;
use zksync_types::{ethabi, Execute, ProtocolVersionId, Transaction, U256};

use super::*;
use crate::storage::ConnectionPool;

fn make_tx<F: contracts::Function>(
    account: &mut Account,
    address: contracts::Address<F::Contract>,
    call: contracts::Call<F>,
) -> Transaction {
    account.get_l2_tx_for_execute(
        Execute {
            contract_address: *address,
            calldata: call.calldata().unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    )
}

pub(crate) struct WeightedValidator {
    weight: validator::Weight,
    key: validator::PublicKey,
    pop: validator::ProofOfPossession,
}

fn encode_attester_key(k: &attester::PublicKey) -> contracts::Secp256k1PublicKey {
    let b: [u8; 33] = ByteFmt::encode(k).try_into().unwrap();
    contracts::Secp256k1PublicKey {
        tag: b[0..1].try_into().unwrap(),
        x: b[1..33].try_into().unwrap(),
    }
}

fn encode_validator_key(k: &validator::PublicKey) -> contracts::BLS12_381PublicKey {
    let b: [u8; 96] = ByteFmt::encode(k).try_into().unwrap();
    contracts::BLS12_381PublicKey {
        a: b[0..32].try_into().unwrap(),
        b: b[32..64].try_into().unwrap(),
        c: b[64..96].try_into().unwrap(),
    }
}

fn encode_validator_pop(pop: &validator::ProofOfPossession) -> contracts::BLS12_381Signature {
    let b: [u8; 48] = ByteFmt::encode(pop).try_into().unwrap();
    contracts::BLS12_381Signature {
        a: b[0..32].try_into().unwrap(),
        b: b[32..48].try_into().unwrap(),
    }
}

fn gen_validator(rng: &mut impl Rng) -> WeightedValidator {
    let k: validator::SecretKey = rng.gen();
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

impl Registry {
    fn deploy(account: &mut Account) -> (Address, Transaction) {
        let tx = account.get_deploy_tx(
            &contracts::ConsensusRegistry::bytecode(),
            None,
            zksync_test_account::TxType::L2,
        );
        (Address::new(tx.address), tx.tx)
    }

    fn add(
        &self,
        node_owner: ethabi::Address,
        validator: WeightedValidator,
        attester: attester::WeightedAttester,
    ) -> anyhow::Result<contracts::Call<contracts::Add>> {
        Ok(self.contract.call(contracts::Add {
            node_owner,
            validator_pub_key: encode_validator_key(&validator.key),
            validator_weight: validator
                .weight
                .try_into()
                .context("overflow")
                .context("validator_weight")?,
            validator_pop: encode_validator_pop(&validator.pop),
            attester_pub_key: encode_attester_key(&attester.key),
            attester_weight: attester
                .weight
                .try_into()
                .context("overflow")
                .context("attester_weight")?,
        }))
    }

    fn initialize(&self, initial_owner: ethabi::Address) -> contracts::Call<contracts::Initialize> {
        self.contract.call(contracts::Initialize { initial_owner })
    }

    fn commit_attester_committee(&self) -> contracts::Call<contracts::CommitAttesterCommittee> {
        self.contract.call(contracts::CommitAttesterCommittee)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_vm_reader() {
    zksync_concurrency::testonly::abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();
    let setup = Setup::new(rng, 1);

    scope::run!(ctx, |ctx, s| async {
        let pool = ConnectionPool::test(false, ProtocolVersionId::latest()).await;
        let registry = Registry::new(setup.genesis.clone(), pool.clone()).await;
        let mut account = Account::random();
        zksync_state_keeper::testonly::fund(&pool.0, &[account.address]).await;

        let (mut node, runner) = crate::testonly::StateKeeper::new(ctx, pool.clone()).await?;
        s.spawn_bg(runner.run_real(ctx));

        // Deploy registry contract and initialize it.
        let address = account.address();
        let (registry_addr, tx) = Registry::deploy(&mut account);
        let mut txs = vec![tx];
        txs.push(make_tx(
            &mut account,
            registry_addr,
            registry.initialize(address),
        ));

        // Configure the registry contract with a new attester committee.
        let committee = attester::Committee::new((0..5).map(|_| gen_attester(rng))).unwrap();
        for a in committee.iter() {
            txs.push(make_tx(
                &mut account,
                registry_addr,
                registry
                    .add(rng.gen(), gen_validator(rng), a.clone())
                    .unwrap(),
            ));
        }
        txs.push(make_tx(
            &mut account,
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
