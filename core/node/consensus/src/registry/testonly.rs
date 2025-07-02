use rand::Rng;
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::validator;
use zksync_test_contracts::Account;
use zksync_types::{ethabi, Execute, Transaction, U256};

use super::*;

pub(crate) fn make_tx<F: crate::abi::Function>(
    account: &mut Account,
    address: crate::abi::Address<F::Contract>,
    call: crate::abi::Call<F>,
) -> Transaction {
    account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(*address),
            calldata: call.calldata().unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeightedValidator {
    pub(crate) is_leader: bool,
    pub(crate) weight: u64,
    pub(crate) key: validator::PublicKey,
    pub(crate) pop: validator::ProofOfPossession,
}

fn encode_validator_key(k: &validator::PublicKey) -> abi::BLS12_381PublicKey {
    let b: [u8; 96] = ByteFmt::encode(k).try_into().unwrap();
    abi::BLS12_381PublicKey {
        a: b[0..32].try_into().unwrap(),
        b: b[32..64].try_into().unwrap(),
        c: b[64..96].try_into().unwrap(),
    }
}

fn encode_validator_pop(pop: &validator::ProofOfPossession) -> abi::BLS12_381Signature {
    let b: [u8; 48] = ByteFmt::encode(pop).try_into().unwrap();
    abi::BLS12_381Signature {
        a: b[0..32].try_into().unwrap(),
        b: b[32..48].try_into().unwrap(),
    }
}

pub(crate) fn gen_validator(rng: &mut impl Rng) -> WeightedValidator {
    let k: validator::SecretKey = rng.gen();
    WeightedValidator {
        is_leader: rng.gen(),
        key: k.public(),
        weight: rng.gen_range(1..100),
        pop: k.sign_pop(),
    }
}

impl Registry {
    pub(crate) fn deploy(account: &mut Account) -> (RegistryAddress, Transaction) {
        let tx = account.get_deploy_tx(
            &abi::ConsensusRegistry::bytecode(),
            None,
            zksync_test_contracts::TxType::L2,
        );
        (RegistryAddress::new(tx.address), tx.tx)
    }

    pub(crate) fn add(
        &self,
        node_owner: ethabi::Address,
        validator: WeightedValidator,
    ) -> anyhow::Result<crate::abi::Call<abi::Add>> {
        Ok(self.contract.call(abi::Add {
            node_owner,
            validator_is_leader: validator.is_leader,
            validator_pub_key: encode_validator_key(&validator.key),
            validator_weight: validator
                .weight
                .try_into()
                .context("overflow")
                .context("validator_weight")?,
            validator_pop: encode_validator_pop(&validator.pop),
        }))
    }

    pub(crate) fn initialize(
        &self,
        initial_owner: ethabi::Address,
    ) -> crate::abi::Call<abi::Initialize> {
        self.contract.call(abi::Initialize { initial_owner })
    }

    pub(crate) fn commit_validator_committee(
        &self,
    ) -> crate::abi::Call<abi::CommitValidatorCommittee> {
        self.contract.call(abi::CommitValidatorCommittee)
    }

    pub(crate) fn set_committee_activation_delay(
        &self,
        delay: u32,
    ) -> crate::abi::Call<abi::SetCommitteeActivationDelay> {
        self.contract
            .call(abi::SetCommitteeActivationDelay { delay })
    }
}
