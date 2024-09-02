use rand::Rng;
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::{attester, validator};
use zksync_contracts::consensus as contracts;
use zksync_test_account::Account;
use zksync_types::{ethabi, Execute, Transaction, U256};

use super::*;

pub(crate) fn make_tx<F: contracts::Function>(
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

pub(crate) fn gen_validator(rng: &mut impl Rng) -> WeightedValidator {
    let k: validator::SecretKey = rng.gen();
    WeightedValidator {
        key: k.public(),
        weight: rng.gen_range(1..100),
        pop: k.sign_pop(),
    }
}

pub(crate) fn gen_attester(rng: &mut impl Rng) -> attester::WeightedAttester {
    attester::WeightedAttester {
        key: rng.gen(),
        weight: rng.gen_range(1..100),
    }
}

impl Registry {
    pub(crate) fn deploy(&self, account: &mut Account) -> (Address, Transaction) {
        let tx = account.get_deploy_tx(
            &contracts::ConsensusRegistry::bytecode(),
            None,
            zksync_test_account::TxType::L2,
        );
        (Address::new(tx.address), tx.tx)
    }

    pub(crate) fn add(
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

    pub(crate) fn initialize(
        &self,
        initial_owner: ethabi::Address,
    ) -> contracts::Call<contracts::Initialize> {
        self.contract.call(contracts::Initialize { initial_owner })
    }

    pub(crate) fn commit_attester_committee(
        &self,
    ) -> contracts::Call<contracts::CommitAttesterCommittee> {
        self.contract.call(contracts::CommitAttesterCommittee)
    }
}
