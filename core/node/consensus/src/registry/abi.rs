//! Strongly-typed API for ConsensusRegistry contract.
#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Context as _;
use zksync_types::{ethabi, ethabi::Token};

use crate::abi;

/// Represents ConsensusRegistry contract.
#[derive(Debug, Clone)]
pub(crate) struct ConsensusRegistry(Arc<ethabi::Contract>);

impl AsRef<ethabi::Contract> for ConsensusRegistry {
    fn as_ref(&self) -> &ethabi::Contract {
        &self.0
    }
}

impl ConsensusRegistry {
    const FILE: &'static str =
        "contracts/l2-contracts/zkout/ConsensusRegistry.sol/ConsensusRegistry.json";

    /// Loads bytecode of the contract.
    #[cfg(test)]
    pub(crate) fn bytecode() -> Vec<u8> {
        zksync_contracts::read_bytecode(Self::FILE)
    }

    /// Loads the `ethabi` representation of the contract.
    pub(crate) fn load() -> Self {
        Self(zksync_contracts::load_contract(ConsensusRegistry::FILE).into())
    }

    /// Constructs a call to function `F` of this contract.
    pub(crate) fn call<F: abi::Function<Contract = Self>>(&self, inputs: F) -> abi::Call<F> {
        abi::Call {
            contract: self.clone(),
            inputs,
        }
    }
}

/// ConsensusRegistry.initialize function.
#[derive(Debug, Default)]
pub(crate) struct Initialize {
    pub(crate) initial_owner: ethabi::Address,
}

impl abi::Function for Initialize {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "initialize";
    fn encode(&self) -> Vec<Token> {
        vec![Token::Address(self.initial_owner)]
    }
    type Outputs = ();
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<()> {
        let [] = tokens.try_into().ok().context("bad size")?;
        Ok(())
    }
}

/// ConsensusRegistry.add function.
#[derive(Debug, Default)]
pub(crate) struct Add {
    pub(crate) node_owner: ethabi::Address,
    pub(crate) validator_weight: u32,
    pub(crate) validator_pub_key: BLS12_381PublicKey,
    pub(crate) validator_pop: BLS12_381Signature,
}

impl abi::Function for Add {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "add";
    fn encode(&self) -> Vec<Token> {
        vec![
            Token::Address(self.node_owner),
            Token::Uint(self.validator_weight.into()),
            self.validator_pub_key.to_token(),
            self.validator_pop.to_token(),
        ]
    }
    type Outputs = ();
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<()> {
        let [] = tokens.try_into().ok().context("bad size")?;
        Ok(())
    }
}

/// ConsensusRegistry.commitValidatorCommittee function.
#[derive(Debug, Default)]
pub(crate) struct CommitValidatorCommittee;

impl abi::Function for CommitValidatorCommittee {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "commitValidatorCommittee";
    fn encode(&self) -> Vec<Token> {
        vec![]
    }
    type Outputs = ();
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<()> {
        let [] = tokens.try_into().ok().context("bad size")?;
        Ok(())
    }
}

/// ConsensusRegistry.getValidatorCommittee function.
#[derive(Debug, Default)]
pub(crate) struct GetValidatorCommittee;

impl abi::Function for GetValidatorCommittee {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "getValidatorCommittee";
    fn encode(&self) -> Vec<Token> {
        vec![]
    }
    type Outputs = (Vec<Validator>, LeaderSelection);
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<Self::Outputs> {
        let [validators, leader_selection] = tokens.try_into().ok().context("bad size")?;
        let mut val = vec![];
        for token in validators.into_array().context("not array")? {
            val.push(Validator::from_token(token).context("validators")?);
        }
        let leader_selection =
            LeaderSelection::from_token(leader_selection).context("leader_selection")?;
        Ok((val, leader_selection))
    }
}

/// ConsensusRegistry.getNextValidatorCommittee function.
#[derive(Debug, Default)]
pub(crate) struct GetNextValidatorCommittee;

impl abi::Function for GetNextValidatorCommittee {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "getNextValidatorCommittee";
    fn encode(&self) -> Vec<Token> {
        vec![]
    }
    type Outputs = (Vec<Validator>, LeaderSelection);
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<Self::Outputs> {
        let [validators, leader_selection] = tokens.try_into().ok().context("bad size")?;
        let mut val = vec![];
        for token in validators.into_array().context("not array")? {
            val.push(Validator::from_token(token).context("validators")?);
        }
        let leader_selection =
            LeaderSelection::from_token(leader_selection).context("leader_selection")?;
        Ok((val, leader_selection))
    }
}

/// ConsensusRegistry.setCommitteeActivationDelay function.
#[derive(Debug, Default)]
pub(crate) struct SetCommitteeActivationDelay {
    pub(crate) delay: u32,
}

impl abi::Function for SetCommitteeActivationDelay {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "setCommitteeActivationDelay";
    fn encode(&self) -> Vec<Token> {
        vec![Token::Uint(self.delay.into())]
    }
    type Outputs = ();
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<()> {
        let [] = tokens.try_into().ok().context("bad size")?;
        Ok(())
    }
}

/// ConsensusRegistry.validatorsCommitBlock function.
#[derive(Debug, Default)]
pub(crate) struct ValidatorsCommitBlock;

impl abi::Function for ValidatorsCommitBlock {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "validatorsCommitBlock";
    fn encode(&self) -> Vec<Token> {
        vec![]
    }
    type Outputs = ethabi::Uint;
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<Self::Outputs> {
        let [block_number] = tokens.try_into().ok().context("bad size")?;
        block_number.into_uint().context("not an uint")
    }
}

/// ConsensusRegistry.owner function.
#[derive(Debug, Default)]
pub(crate) struct Owner;

impl abi::Function for Owner {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "owner";
    fn encode(&self) -> Vec<Token> {
        vec![]
    }
    type Outputs = ethabi::Address;
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<Self::Outputs> {
        let [owner] = tokens.try_into().ok().context("bad size")?;
        owner.into_address().context("not an address")
    }
}

// Auxiliary structs.

/// Raw representation of a validator committee member.
#[derive(Debug)]
pub(crate) struct Validator {
    pub(crate) leader: bool,
    pub(crate) weight: u32,
    pub(crate) pub_key: BLS12_381PublicKey,
    pub(crate) proof_of_possession: BLS12_381Signature,
}

impl Validator {
    fn from_token(token: Token) -> anyhow::Result<Self> {
        let [leader, weight, pub_key, proof_of_possession] = abi::into_tuple(token)?;
        Ok(Self {
            leader: abi::into_bool(leader).context("leader")?,
            weight: abi::into_uint(weight).context("weight")?,
            pub_key: BLS12_381PublicKey::from_token(pub_key).context("pub_key")?,
            proof_of_possession: BLS12_381Signature::from_token(proof_of_possession)
                .context("proof_of_possession")?,
        })
    }
}

/// Raw representation of a leader selection parameters.
#[derive(Debug, Default)]
pub(crate) struct LeaderSelection {
    pub(crate) frequency: u64,
    pub(crate) weighted: bool,
}

impl LeaderSelection {
    fn from_token(token: Token) -> anyhow::Result<Self> {
        let [frequency, weighted] = abi::into_tuple(token)?;
        Ok(Self {
            frequency: abi::into_uint(frequency).context("frequency")?,
            weighted: abi::into_bool(weighted).context("weighted")?,
        })
    }
}

/// Raw representation of a BLS12_381 public key.
#[derive(Debug, Default)]
pub(crate) struct BLS12_381PublicKey {
    pub(crate) a: [u8; 32],
    pub(crate) b: [u8; 32],
    pub(crate) c: [u8; 32],
}

impl BLS12_381PublicKey {
    fn from_token(token: Token) -> anyhow::Result<Self> {
        let [a, b, c] = abi::into_tuple(token)?;
        Ok(Self {
            a: abi::into_fixed_bytes(a).context("a")?,
            b: abi::into_fixed_bytes(b).context("b")?,
            c: abi::into_fixed_bytes(c).context("c")?,
        })
    }

    fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.a.into()),
            Token::FixedBytes(self.b.into()),
            Token::FixedBytes(self.c.into()),
        ])
    }
}

#[derive(Debug, Default)]
pub(crate) struct BLS12_381Signature {
    pub(crate) a: [u8; 32],
    pub(crate) b: [u8; 16],
}

impl BLS12_381Signature {
    fn from_token(token: Token) -> anyhow::Result<Self> {
        let [a, b] = abi::into_tuple(token)?;
        Ok(Self {
            a: abi::into_fixed_bytes(a).context("a")?,
            b: abi::into_fixed_bytes(b).context("b")?,
        })
    }

    fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.a.into()),
            Token::FixedBytes(self.b.into()),
        ])
    }
}
