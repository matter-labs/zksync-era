use ethabi::{ParamType,Token};
use anyhow::Context as _;

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

pub struct Call<'a,O> {
    address: ethabi::Address,
    function: &'a ethabi::Function,
    inputs: Vec<Token>,
    _marker: std::marker::PhantomData<O>,
}

pub trait Outputs : Sized { fn from_tokens(tokens: Vec<Token>) -> anyhow::Result<Self>; }

impl<'a,O:Outputs> Call<'a,O> {
    pub fn address(&self) -> ethabi::Address { self.address } 
    pub fn calldata(&self) -> ethabi::Result<ethabi::Bytes> {
        self.function.encode_input(&self.inputs)
    }
    pub fn decode_outputs(&self, outputs: &[u8]) -> anyhow::Result<O> {
        O::from_tokens(self.function.decode_output(outputs).context("decode_output()")?)
    }
    pub fn test(&self) -> anyhow::Result<()> {
        self.calldata().context("calldata")?;
        O::from_tokens(self.function.outputs.iter().map(|p|example(&p.kind)).collect()).context("from_tokens()")?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ConsensusRegistry(ethabi::Contract,ethabi::Address);

#[derive(Debug)]
pub struct GetAttesterCommitteeOutputs(pub Vec<Attester>);

impl Outputs for GetAttesterCommitteeOutputs {
    fn from_tokens(tokens: Vec<Token>) -> anyhow::Result<Self> {
        let [attesters] = tokens.try_into().ok().context("bad size")?; 
        let mut res = vec![];
        for token in attesters.into_array().context("not array")? {
            res.push(Attester::from_token(token).context("attesters")?);
        }
        Ok(Self(res))
    }
}

#[derive(Debug)]
pub struct AddOutputs;

impl Outputs for AddOutputs {
    fn from_tokens(tokens: Vec<Token>) -> anyhow::Result<Self> {
        let [] = tokens.try_into().ok().context("bad size")?;
        Ok(Self)
    }
}

#[derive(Debug)]
pub struct InitializeOutputs;

impl Outputs for InitializeOutputs {
    fn from_tokens(tokens : Vec<Token>) -> anyhow::Result<Self> { 
        let [] = tokens.try_into().ok().context("bad size")?;
        Ok(Self)
    }
}

impl ConsensusRegistry {
    const FILE: &str = "contracts/l2-contracts/artifacts-zk/contracts/ConsensusRegistry.sol/ConsensusRegistry.json";
    pub fn bytecode() -> Vec<u8> {
        crate::read_bytecode(Self::FILE)
    }
    pub fn at(address: ethabi::Address) -> Self {
        Self(crate::load_contract(ConsensusRegistry::FILE),address)
    }
    fn call<O>(&self, name: &'static str, inputs: Vec<Token>) -> Call<O> {
        let function = self.0.function(name).unwrap();
        Call {
            address: self.1,
            inputs: inputs,
            function,
            _marker: std::marker::PhantomData,
        }
    }
    pub fn get_attester_committee(&self) -> Call<GetAttesterCommitteeOutputs> {
        self.call("getAttesterCommittee",vec![])
    }
    pub fn add(&self,
        node_owner: ethabi::Address,
        validator_weight: u32,
        validator_pub_key: BLS12_381PublicKey,
        validator_pop: BLS12_381Signature,
        attester_weight: u32,
        attester_pub_key: Secp256k1PublicKey,
    ) -> Call<AddOutputs> {
        self.call("add", vec![
            Token::Address(node_owner),
            Token::Uint(validator_weight.into()),
            validator_pub_key.into_token(),
            validator_pop.into_token(),
            Token::Uint(attester_weight.into()),
            attester_pub_key.into_token(),
        ])
    }
    pub fn initialize(&self, initial_owner: ethabi::Address) -> Call<InitializeOutputs> {
        self.call("initialize", vec![Token::Address(initial_owner)])
    }
}

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

    fn into_token(self) -> Token {
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
    fn into_token(self) -> Token {
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
    fn into_token(self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.a.into()),
            Token::FixedBytes(self.b.into()),
        ])
    }
}
