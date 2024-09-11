//! Strongly-typed API for ConsensusRegistry contract.
#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Context as _;
use zksync_types::{ethabi, ethabi::Token};

use crate::abi;

/// Reprents ConsensusRegistry contract.
#[derive(Debug, Clone)]
pub(crate) struct ConsensusRegistry(Arc<ethabi::Contract>);

impl AsRef<ethabi::Contract> for ConsensusRegistry {
    fn as_ref(&self) -> &ethabi::Contract {
        &self.0
    }
}

impl ConsensusRegistry {
    const FILE: &'static str = "contracts/l2-contracts/artifacts-zk/contracts/ConsensusRegistry.sol/ConsensusRegistry.json";

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

/// ConsensusRegistry.getAttesterCommittee function.
#[derive(Debug, Default)]
pub(crate) struct GetAttesterCommittee;

impl abi::Function for GetAttesterCommittee {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "getAttesterCommittee";

    fn encode(&self) -> Vec<Token> {
        vec![]
    }

    type Outputs = Vec<Attester>;
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<Self::Outputs> {
        let [attesters] = tokens.try_into().ok().context("bad size")?;
        let mut res = vec![];
        for token in attesters.into_array().context("not array")? {
            res.push(Attester::from_token(token).context("attesters")?);
        }
        Ok(res)
    }
}

/// ConsensusRegistry.add function.
#[derive(Debug, Default)]
pub(crate) struct Add {
    pub(crate) node_owner: ethabi::Address,
    pub(crate) validator_weight: u32,
    pub(crate) validator_pub_key: BLS12_381PublicKey,
    pub(crate) validator_pop: BLS12_381Signature,
    pub(crate) attester_weight: u32,
    pub(crate) attester_pub_key: Secp256k1PublicKey,
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
            Token::Uint(self.attester_weight.into()),
            self.attester_pub_key.to_token(),
        ]
    }
    type Outputs = ();
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<()> {
        let [] = tokens.try_into().ok().context("bad size")?;
        Ok(())
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

/// ConsensusRegistry.commitAttesterCommittee function.
#[derive(Debug, Default)]
pub(crate) struct CommitAttesterCommittee;

impl abi::Function for CommitAttesterCommittee {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "commitAttesterCommittee";
    fn encode(&self) -> Vec<Token> {
        vec![]
    }
    type Outputs = ();
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<()> {
        let [] = tokens.try_into().ok().context("bad size")?;
        Ok(())
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

/// Raw representation of a secp256k1 public key.
#[derive(Debug, Default)]
pub(crate) struct Secp256k1PublicKey {
    pub(crate) tag: [u8; 1],
    pub(crate) x: [u8; 32],
}

impl Secp256k1PublicKey {
    fn from_token(token: Token) -> anyhow::Result<Self> {
        let [tag, x] = abi::into_tuple(token)?;
        Ok(Self {
            tag: abi::into_fixed_bytes(tag).context("tag")?,
            x: abi::into_fixed_bytes(x).context("x")?,
        })
    }

    fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.tag.into()),
            Token::FixedBytes(self.x.into()),
        ])
    }
}

/// Raw representation of an attester committee member.
#[derive(Debug)]
pub(crate) struct Attester {
    pub(crate) weight: u32,
    pub(crate) pub_key: Secp256k1PublicKey,
}

impl Attester {
    fn from_token(token: Token) -> anyhow::Result<Self> {
        let [weight, pub_key] = abi::into_tuple(token)?;
        Ok(Self {
            weight: abi::into_uint(weight).context("weight")?,
            pub_key: Secp256k1PublicKey::from_token(pub_key).context("pub_key")?,
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
    fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.a.into()),
            Token::FixedBytes(self.b.into()),
        ])
    }
}
