use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _};
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::{attester, validator};

use crate::{storage::ConnectionPool, vm::VM};

mod abi;
#[cfg(test)]
pub(crate) mod testonly;
#[cfg(test)]
mod tests;

fn decode_attester_key(k: &abi::Secp256k1PublicKey) -> anyhow::Result<attester::PublicKey> {
    let mut x = vec![];
    x.extend(k.tag);
    x.extend(k.x);
    ByteFmt::decode(&x)
}

fn decode_weighted_attester(a: &abi::Attester) -> anyhow::Result<attester::WeightedAttester> {
    Ok(attester::WeightedAttester {
        weight: a.weight.into(),
        key: decode_attester_key(&a.pub_key).context("key")?,
    })
}

pub type Address = crate::abi::Address<abi::ConsensusRegistry>;

#[derive(Debug)]
pub(crate) struct Registry {
    contract: abi::ConsensusRegistry,
    genesis: validator::Genesis,
    vm: VM,
}

impl Registry {
    pub async fn new(genesis: validator::Genesis, pool: ConnectionPool) -> Self {
        Self {
            contract: abi::ConsensusRegistry::load(),
            genesis,
            vm: VM::new(pool).await,
        }
    }

    /// Attester committee for the given batch.
    /// It reads committee from the contract.
    /// Falls back to committee specified in the genesis.
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
            return Ok(self.genesis.attesters.clone());
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
