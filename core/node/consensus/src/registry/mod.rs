use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _};
use zksync_consensus_crypto::ByteFmt;
use zksync_consensus_roles::validator;

use crate::{
    storage::ConnectionPool,
    vm::{VMResult, VM},
};

mod abi;
#[cfg(test)]
pub(crate) mod testonly;
#[cfg(test)]
mod tests;

pub(crate) type RegistryAddress = crate::abi::Address<abi::ConsensusRegistry>;

#[derive(Debug)]
pub(crate) struct Registry {
    contract: abi::ConsensusRegistry,
    address: RegistryAddress,
    vm: VM,
}

impl Registry {
    pub async fn new(pool: ConnectionPool, address: RegistryAddress) -> Self {
        Self {
            contract: abi::ConsensusRegistry::load(),
            address,
            vm: VM::new(pool).await,
        }
    }

    /// It gets a current validator committee from the consensus registry contract.
    /// Returns a tuple of (Schedule, commit_block_number).
    pub async fn get_current_validator_schedule(
        &self,
        ctx: &ctx::Ctx,
        sealed_block_number: validator::BlockNumber,
    ) -> ctx::Result<(validator::Schedule, validator::BlockNumber)> {
        self.get_schedule_inner(ctx, sealed_block_number, true)
            .await
            .context("get_schedule()")?
            .ok_or(anyhow::anyhow!("failed to get current validator schedule").into())
    }

    /// It tries to get a pending validator committee from the consensus registry contract.
    /// Returns `None` if there's no pending committee.
    /// If a pending committee exists, returns a tuple of (Schedule, commit_block_number).
    pub async fn get_pending_validator_schedule(
        &self,
        ctx: &ctx::Ctx,
        sealed_block_number: validator::BlockNumber,
    ) -> ctx::Result<Option<(validator::Schedule, validator::BlockNumber)>> {
        self.get_schedule_inner(ctx, sealed_block_number, false)
            .await
    }

    async fn get_schedule_inner(
        &self,
        ctx: &ctx::Ctx,
        sealed_block_number: validator::BlockNumber,
        cur_schedule_flag: bool,
    ) -> ctx::Result<Option<(validator::Schedule, validator::BlockNumber)>> {
        let (validators, leader_selection) = if cur_schedule_flag {
            match self
                .vm
                .call(
                    ctx,
                    sealed_block_number,
                    self.address,
                    self.contract.call(abi::GetValidatorCommittee),
                )
                .await
                .wrap("vm.call()")?
            {
                VMResult::Success { output } => output,
                // This should never happen as there are no reverts in this contract method.
                VMResult::Revert { revert_reason } => {
                    return Err(anyhow::anyhow!("revert: {revert_reason}").into());
                }
            }
        } else {
            match self
                .vm
                .call(
                    ctx,
                    sealed_block_number,
                    self.address,
                    self.contract.call(abi::GetNextValidatorCommittee),
                )
                .await
                .wrap("vm.call()")?
            {
                VMResult::Success { output } => output,
                // The only possible revert is if there's no pending committee, so we just return `None`.
                // We still log this, just in case.
                VMResult::Revert { revert_reason } => {
                    tracing::debug!("no pending committee: {revert_reason}");
                    return Ok(None);
                }
            }
        };

        // If there are no validators in the committee, error. However, this should never happen
        // as the contract checks that there is at least one leader validator before committing
        // a new committee. So this is just a sanity check.
        if validators.is_empty() {
            return Err(anyhow::anyhow!("no validators in the committee").into());
        }

        let mut validator_infos = vec![];
        for a in validators {
            // TODO: This is a temporary fix to handle the case where the validator is not valid.
            // We should remove this once we can verify the validator's proof of possession on-chain.
            match decode_weighted_validator(&a) {
                Ok(validator_info) => {
                    validator_infos.push(validator_info);
                }
                Err(err) => {
                    tracing::warn!("Failed to decode weighted validator: {:?}", err);
                }
            }
        }

        let leader_selection = validator::LeaderSelection {
            frequency: leader_selection.frequency,
            mode: if leader_selection.weighted {
                validator::LeaderSelectionMode::Weighted
            } else {
                validator::LeaderSelectionMode::RoundRobin
            },
        };

        let schedule = validator::Schedule::new(validator_infos.into_iter(), leader_selection)
            .context("Schedule::new()")?;

        // Get the validators commit block
        let commit_block_uint = match self
            .vm
            .call(
                ctx,
                sealed_block_number,
                self.address,
                self.contract.call(abi::ValidatorsCommitBlock),
            )
            .await
            .wrap("get_validators_commit_block()")?
        {
            VMResult::Success { output } => output,
            VMResult::Revert { revert_reason } => {
                return Err(anyhow::anyhow!("revert: {revert_reason}").into());
            }
        };

        let commit_block = validator::BlockNumber(commit_block_uint.as_u64());

        Ok(Some((schedule, commit_block)))
    }
}

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

fn decode_weighted_validator(v: &abi::Validator) -> anyhow::Result<validator::ValidatorInfo> {
    let key = decode_validator_key(&v.pub_key).context("key")?;

    let pop = decode_validator_pop(&v.proof_of_possession).context("proof of possession")?;

    pop.verify(&key).context("verify proof of possession")?;

    Ok(validator::ValidatorInfo {
        weight: v.weight.into(),
        key,
        leader: v.leader,
    })
}
