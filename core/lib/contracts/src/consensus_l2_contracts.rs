use ethabi::{ParamType,Contract,Token};
use anyhow::Context as _;

pub struct ConsensusRegistry(Contract);

pub trait FunctionSig {
    const NAME: &'static str;
    type Inputs : Default;
    type Outputs;
    fn encode_inputs(inputs: Self::Inputs) -> Vec<Token>;
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<Self::Outputs>;
}

pub struct GetAttesterCommittee;

impl FunctionSig for GetAttesterCommittee {
    const NAME : &'static str = "getAttesterCommittee";
    type Inputs = ();
    type Outputs = Vec<Attester>;
    fn encode_inputs(_: Self::Inputs) -> Vec<Token> { vec![] }
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<Self::Outputs> {
        let [attesters] = tokens.try_into().ok().context("bad size")?; 
        let mut res = vec![];
        for token in attesters.into_array().context("not array")? {
            res.push(Attester::from_token(token).context("attesters")?);
        }
        Ok(res)
    }
}

pub struct Add;

impl FunctionSig for Add {
    const NAME : &'static str = "add";
    type Inputs = AddInputs;
    type Outputs = ();
    fn encode_inputs(inputs: Self::Inputs) -> Vec<Token> {
        vec![
            Token::Address(inputs.node_owner),
            Token::Uint(inputs.validator_weight.into()),
            inputs.validator_pub_key.into_token(),
            inputs.validator_pop.into_token(),
            Token::Uint(inputs.attester_weight.into()),
            inputs.attester_pub_key.into_token(),
        ]
    }
    fn decode_outputs(tokens: Vec<Token>) -> anyhow::Result<Self::Outputs> {
        let [] = tokens.try_into().ok().context("bad size")?;
        Ok(())
    }
}

pub struct Function<'a, Sig>(&'a Contract, std::marker::PhantomData<Sig>);

impl<'a, Sig> From<&'a Contract> for Function<'a, Sig> {
    fn from(c: &'a Contract) -> Self { Self(c,std::marker::PhantomData) }
}

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

impl<'a, Sig: FunctionSig> Function<'a, Sig> {
    pub fn encode_input(&self, inputs: Sig::Inputs) -> ethabi::Result<ethabi::Bytes> {
        self.0.function(Sig::NAME).unwrap().encode_input(&Sig::encode_inputs(inputs))
    }
    pub fn decode_output(&self, outputs: &[u8]) -> ethabi::Result<Sig::Outputs> {
        let outputs = self.0.function(Sig::NAME).unwrap().decode_output(outputs)?;
        Sig::decode_outputs(outputs).map_err(|err|ethabi::Error::Other(err.to_string().into()))
    }

    pub fn test(&self) -> anyhow::Result<()> {
        let f = self.0.function(Sig::NAME).context("function()")?;
        Sig::decode_outputs(f.outputs.iter().map(|p|example(&p.kind)).collect()).context("Sig::decode_outputs()")?;
        f.encode_input(&Sig::encode_inputs(Sig::Inputs::default())).context("f.encode_intput()")?;
        Ok(())
    }
}

impl ConsensusRegistry {
    const FILE: &str = "contracts/l2-contracts/artifacts-zk/contracts/ConsensusRegistry.sol/ConsensusRegistry.json";
    
    pub fn load() -> Self {
        Self(crate::load_contract(Self::FILE))
    }

    pub fn bytecode() -> Vec<u8> {
        crate::read_bytecode(Self::FILE)
    }

    pub fn get_attester_committee(&self) -> Function<GetAttesterCommittee> { (&self.0).into() }

    pub fn add(&self) -> Function<Add> { (&self.0).into() }
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

#[derive(Debug, Default)]
pub struct AddInputs {
    node_owner: ethabi::Address,
    validator_weight: u32,
    validator_pub_key: BLS12_381PublicKey,
    validator_pop: BLS12_381Signature,
    attester_weight: u32,
    attester_pub_key: Secp256k1PublicKey,
}
