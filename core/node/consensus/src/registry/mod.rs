use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _};
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::validator;

use crate::{storage::ConnectionPool, vm::VM};

mod abi;
#[cfg(test)]
pub(crate) mod testonly;
#[cfg(test)]
mod tests;

fn decode_validator_key(k: &abi::BLS12_381PublicKey) -> anyhow::Result<validator::PublicKey> {
    let mut x = vec![];
    x.extend(k.a);
    x.extend(k.b);
    x.extend(k.c);
    ByteFmt::decode(&x)
}

fn decode_validator_pop(
    pop: &abi::BLS12_381Signature,
) -> anyhow::Result<validator::ProofOfPossession> {
    let mut bytes = vec![];
    bytes.extend(pop.a);
    bytes.extend(pop.b);
    ByteFmt::decode(&bytes).context("decode proof of possession")
}

fn decode_weighted_validator(v: &abi::Validator) -> anyhow::Result<validator::WeightedValidator> {
    let key = decode_validator_key(&v.pub_key).context("key")?;

    let pop = decode_validator_pop(&v.proof_of_possession).context("proof of possession")?;

    pop.verify(&key).context("verify proof of possession")?;

    Ok(validator::WeightedValidator {
        weight: v.weight.into(),
        key,
    })
}

pub type Address = crate::abi::Address<abi::ConsensusRegistry>;

#[derive(Debug)]
pub(crate) struct Registry {
    contract: abi::ConsensusRegistry,
    vm: VM,
}

impl Registry {
    pub async fn new(pool: ConnectionPool) -> Self {
        Self {
            contract: abi::ConsensusRegistry::load(),
            vm: VM::new(pool).await,
        }
    }

    /// Attester committee for the given batch.
    /// It reads committee from the contract.
    /// Falls back to empty committee.
    pub async fn attester_committee_for(
        &self,
        ctx: &ctx::Ctx,
        address: Option<Address>,
        attested_batch: attester::BatchNumber,
    ) -> ctx::Result<Option<attester::Committee>> {
        let Some(batch_defining_committee) = attested_batch.prev() else {
            // Batch 0 doesn't need attestation.
            return Ok(None);
        };
        let Some(address) = address else {
            return Ok(None);
        };
        let raw = self
            .vm
            .call(
                ctx,
                batch_defining_committee,
                address,
                self.contract.call(abi::GetAttesterCommittee),
            )
            .await
            .wrap("vm.call()")?;
        let mut attesters = vec![];
        for a in raw {
            attesters.push(decode_weighted_attester(&a).context("decode_weighted_attester()")?);
        }
        Ok(Some(
            attester::Committee::new(attesters.into_iter()).context("Committee::new()")?,
        ))
    }
}
