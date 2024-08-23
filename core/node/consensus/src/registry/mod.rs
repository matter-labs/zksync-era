use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _};
use zksync_contracts::consensus as contracts;
use zksync_consensus_roles::attester;
use zksync_consensus_crypto::ByteFmt;
use zksync_types::ethabi;
use crate::vm::VM;

#[cfg(test)]
mod tests;

fn decode_attester_key(k: &contracts::Secp256k1PublicKey) -> anyhow::Result<attester::PublicKey> {
    let mut x = vec![];
    x.extend(k.tag);
    x.extend(k.x);
    ByteFmt::decode(&x) 
}

fn decode_weighted_attester(a: &contracts::Attester) -> anyhow::Result<attester::WeightedAttester> {
    Ok(attester::WeightedAttester {
        weight: a.weight.into(),
        key: decode_attester_key(&a.pub_key).context("key")?,
    })
}

pub(crate) struct Contract(contracts::ConsensusRegistry);

impl Contract {
    pub fn at(address: ethabi::Address) -> Self {
        Self(contracts::ConsensusRegistry::at(address))
    }

    /// Reads attester committee from the registry contract.
    pub async fn get_attester_committee(&self, ctx: &ctx::Ctx, vm: &VM, batch: attester::BatchNumber) -> ctx::Result<attester::Committee> {
        let raw = vm.call(ctx, batch, self.0.call(contracts::GetAttesterCommittee)).await.wrap("vm.call()")?;
        let mut attesters = vec![];
        for a in raw {
           attesters.push(decode_weighted_attester(&a).context("decode_weighted_attester()")?);
        }
        Ok(attester::Committee::new(attesters.into_iter()).context("Committee::new()")?)
    }
}
