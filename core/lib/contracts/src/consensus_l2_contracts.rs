use ethabi::{ParamType,Token};
use anyhow::Context as _;
use std::sync::Arc;

fn example(t: &ParamType) -> Token {
    use ParamType as T;
    match t {
        T::Address => Token::Address(ethabi::Address::default()),
        T::Bytes => Token::Bytes(ethabi::Bytes::default()),
        T::Int(_) => Token::Int(ethabi::Int::default()),
        T::Uint(_) => Token::Uint(ethabi::Uint::default()),
        T::Bool => Token::Bool(bool::default()),
        T::String => Token::String(String::default()),
        T::Array(t) => Token::Array(vec![example(t)]),
        T::FixedBytes(n) => Token::FixedBytes(vec![0;*n]),
        T::FixedArray(t, n) => Token::FixedArray(vec![example(t);*n]),
        T::Tuple(ts) => Token::Tuple(ts.iter().map(example).collect()),
    }
}

pub struct DeployedContract {
    pub address: ethabi::Address,
    pub contract: ethabi::Contract,
}

pub trait Function : Default {
    const NAME: &'static str;
    type Contract: AsRef<DeployedContract>;
    type Outputs;
    fn encode(&self) -> Vec<Token>;
    fn decode_outputs(outputs: &[u8]) -> anyhow::Result<Self::Outputs>;
}

pub struct Call<F> {
    pub contract: F::Contract,
    pub inputs: F,
}

impl<F:Function> Call<F> {
    pub fn address(&self) -> ethabi::Address { self.address } 
    pub fn calldata(&self) -> ethabi::Result<ethabi::Bytes> {
        self.function.encode_input(&self.inputs)
    }
    pub fn decode_outputs(&self, outputs: &[u8]) -> anyhow::Result<F::Outputs> {
        O::from_tokens(self.function.decode_output(outputs).context("decode_output()")?)
    }
    pub fn test(&self) -> anyhow::Result<()> {
        self.calldata().context("calldata")?;
        O::from_tokens(self.function.outputs.iter().map(|p|example(&p.kind)).collect()).context("from_tokens()")?;
        Ok(())
    }
}

#[derive(Debug,Clone)]
pub struct ConsensusRegistry(Arc<DeployedContract>);

impl AsRef<DeployedContract> {
    fn as_ref(&self) -> &DeployedContract { &self.0 }
}

impl ConsensusRegistry {
    const FILE: &str = "contracts/l2-contracts/artifacts-zk/contracts/ConsensusRegistry.sol/ConsensusRegistry.json";
    pub fn bytecode() -> Vec<u8> {
        crate::read_bytecode(Self::FILE)
    }
    pub fn at(address: ethabi::Address) -> Self {
        Self(DeployedContract {
            contract: crate::load_contract(ConsensusRegistry::FILE)
            address,
        }.into())
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
        Ok(Self)
    }
}

#[derive(Debug)]
pub struct Initialize {
    initial_owner: ethabi::Address,
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
        Ok(Self)
    }
}

// Auxiliary structs.

#[derive(Debug, Default)]
pub struct Secp256k1PublicKey {
    pub tag: [u8;1],
    pub x: [u8;32],
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
    a: [u8;32],
    b: [u8;32],
    c: [u8;32],
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
    a: [u8;32],
    b: [u8;16],
}

impl BLS12_381Signature {
    fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.a.into()),
            Token::FixedBytes(self.b.into()),
        ])
    }
}
