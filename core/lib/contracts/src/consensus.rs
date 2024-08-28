use ethabi::{Token};
use anyhow::Context as _;
use std::sync::Arc;

pub trait Function {
    const NAME: &'static str;
    type Contract: AsRef<ethabi::Contract>;
    type Outputs;
    fn encode(&self) -> Vec<Token>;
    fn decode_outputs(outputs: Vec<Token>) -> anyhow::Result<Self::Outputs>;
}

/// Address of contract C. It is just a wrapper of ethabi::Address.
#[derive(Debug,Hash)]
pub struct Address<C>(ethabi::Address,std::marker::PhantomData<C>);

impl<C> Clone for Address<C> {
    fn clone(&self) -> Self { Self(self.0,self.1) }
}

impl<C> Copy for Address<C> {}

impl<C> PartialEq for Address<C> {
    fn eq(&self,other:&Self) -> bool { self.0.eq(&other.0) }
}

impl<C> Eq for Address<C> {}

impl<C> Address<C> {
    pub fn new(address: ethabi::Address) -> Self {
        Self(address,std::marker::PhantomData)
    }
}

impl<C> std::ops::Deref for Address<C> {
    type Target = ethabi::Address;
    fn deref(&self) -> &Self::Target { &self.0 }
}

pub struct Call<F:Function> {
    pub contract: F::Contract,
    pub inputs: F,
}

impl<F:Function> Call<F> {
    pub(crate) fn function(&self) -> &ethabi::Function { self.contract.as_ref().function(F::NAME).unwrap() }
    pub fn calldata(&self) -> ethabi::Result<ethabi::Bytes> {
        self.function().encode_input(&self.inputs.encode())
    }
    pub fn decode_outputs(&self, outputs: &[u8]) -> anyhow::Result<F::Outputs> {
        F::decode_outputs(self.function().decode_output(outputs).context("decode_output()")?)
    } 
}

fn into_fixed_bytes<const N: usize>(t: Token) -> anyhow::Result<[u8;N]> {
    match t {
        Token::FixedBytes(b) => b.try_into().ok().context("bad size"),
        bad => anyhow::bail!("want fixed_bytes, got {bad:?}"),
    }
}

fn into_tuple<const N: usize>(t: Token) -> anyhow::Result<[Token;N]> {
    match t {
        Token::Tuple(ts) => ts.try_into().ok().context("bad size"),
        bad => anyhow::bail!("want tuple, got {bad:?}"),
    }
}

fn into_uint<I:TryFrom<ethabi::Uint>>(t: Token) -> anyhow::Result<I> {
    match t {
        Token::Uint(i) => i.try_into().ok().context("overflow"),
        bad => anyhow::bail!("want uint, got {bad:?}"),
    }
}

#[derive(Debug,Clone)]
pub struct ConsensusRegistry(Arc<ethabi::Contract>);

impl AsRef<ethabi::Contract> for ConsensusRegistry {
    fn as_ref(&self) -> &ethabi::Contract { &self.0 }
}

impl ConsensusRegistry {
    const FILE: &str = "contracts/l2-contracts/artifacts-zk/contracts/ConsensusRegistry.sol/ConsensusRegistry.json";
    pub fn bytecode() -> Vec<u8> {
        crate::read_bytecode(Self::FILE)
    }
    pub fn load() -> Self {
        Self(crate::load_contract(ConsensusRegistry::FILE).into())
    }
    pub fn call<F:Function<Contract=Self>>(&self, inputs: F) -> Call<F> {
        Call { contract: self.clone(), inputs }
    }
}

// Functions.

#[derive(Debug,Default)]
pub struct GetAttesterCommittee;

impl Function for GetAttesterCommittee {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "getAttesterCommittee";
    
    fn encode(&self) -> Vec<Token> { vec![] }
    
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

#[derive(Debug,Default)]
pub struct Add {
    pub node_owner: ethabi::Address,
    pub validator_weight: u32,
    pub validator_pub_key: BLS12_381PublicKey,
    pub validator_pop: BLS12_381Signature,
    pub attester_weight: u32,
    pub attester_pub_key: Secp256k1PublicKey,
}

impl Function for Add {
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

#[derive(Debug,Default)]
pub struct Initialize {
    pub initial_owner: ethabi::Address,
}

impl Function for Initialize {
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

pub struct CommitAttesterCommittee;

impl Function for CommitAttesterCommittee {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "commitAttesterCommittee";
    fn encode(&self) -> Vec<Token> { vec![] }
    type Outputs = ();
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<()> {
        let [] = tokens.try_into().ok().context("bad size")?;
        Ok(())
    }
}

pub struct Owner;

impl Function for Owner {
    type Contract = ConsensusRegistry;
    const NAME: &'static str = "owner";
    fn encode(&self) -> Vec<Token> { vec![] }
    type Outputs = ethabi::Address;
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<Self::Outputs> {
        let [owner] = tokens.try_into().ok().context("bad size")?;
        owner.into_address().context("not an address")
    }
}

// Auxiliary structs.

#[derive(Debug, Default)]
pub struct Secp256k1PublicKey {
    pub tag: [u8;1],
    pub x: [u8;32],
}

impl Secp256k1PublicKey {
    fn from_token(token: Token) -> anyhow::Result<Self> {
        let [tag, x] = into_tuple(token)?;
        Ok(Self {
            tag: into_fixed_bytes(tag).context("tag")?,
            x: into_fixed_bytes(x).context("x")?,
        })
    }

    fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.tag.into()),
            Token::FixedBytes(self.x.into()),
        ])
    }
}

#[derive(Debug)]
pub struct Attester {
    pub weight: u32,
    pub pub_key: Secp256k1PublicKey,
}

impl Attester {
    fn from_token(token: Token) -> anyhow::Result<Self> {
        let [weight, pub_key] = into_tuple(token)?;
        Ok(Self {
            weight: into_uint(weight).context("weight")?,
            pub_key: Secp256k1PublicKey::from_token(pub_key).context("pub_key")?,
        })
    }
}

#[derive(Debug, Default)]
pub struct BLS12_381PublicKey {
    pub a: [u8;32],
    pub b: [u8;32],
    pub c: [u8;32],
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
pub struct BLS12_381Signature {
    pub a: [u8;32],
    pub b: [u8;16],
}

impl BLS12_381Signature {
    fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.a.into()),
            Token::FixedBytes(self.b.into()),
        ])
    }
}
