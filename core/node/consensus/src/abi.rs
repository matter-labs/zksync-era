//! Strongly-typed API for Consensus-related solidity contracts.
//! Placeholder until we can depend on alloy_sol_types.
use anyhow::Context as _;
use zksync_types::{ethabi, ethabi::Token};

/// Strongly typed representation of a contract function.
/// It also represents the inputs of the function.
pub trait Function {
    /// Name of the solidity function.
    const NAME: &'static str;
    /// Type representing contract this function belongs to.
    type Contract: AsRef<ethabi::Contract>;
    /// Typ representing outputs of this function.
    type Outputs;
    /// Encodes this struct to inputs of this function.
    fn encode(&self) -> Vec<Token>;
    /// Decodes outputs of this function.
    fn decode_outputs(outputs: Vec<Token>) -> anyhow::Result<Self::Outputs>;
}

/// Address of contract C. It is just a wrapper of ethabi::Address,
/// just additionally indicating what contract is deployed under this address.
#[derive(Debug)]
pub struct Address<C>(ethabi::Address, std::marker::PhantomData<C>);

impl<C> Clone for Address<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for Address<C> {}

impl<C> PartialEq for Address<C> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<C> Eq for Address<C> {}

impl<C> Address<C> {
    pub fn new(address: ethabi::Address) -> Self {
        Self(address, std::marker::PhantomData)
    }
}

impl<C> std::ops::Deref for Address<C> {
    type Target = ethabi::Address;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Represents a call to the function F.
#[derive(Debug)]
pub struct Call<F: Function> {
    /// Contract of the function.
    pub contract: F::Contract,
    /// Inputs to the function.
    pub inputs: F,
}

impl<F: Function> Call<F> {
    pub(super) fn function(&self) -> &ethabi::Function {
        self.contract.as_ref().function(F::NAME).unwrap()
    }
    /// Converts the call to raw calldata.
    pub fn calldata(&self) -> ethabi::Result<ethabi::Bytes> {
        self.function().encode_input(&self.inputs.encode())
    }
    /// Parses the outputs of the call.
    pub fn decode_outputs(&self, outputs: &[u8]) -> anyhow::Result<F::Outputs> {
        F::decode_outputs(
            self.function()
                .decode_output(outputs)
                .context("decode_output()")?,
        )
    }
}

pub(crate) fn into_fixed_bytes<const N: usize>(t: Token) -> anyhow::Result<[u8; N]> {
    match t {
        Token::FixedBytes(b) => b.try_into().ok().context("bad size"),
        bad => anyhow::bail!("want fixed_bytes, got {bad:?}"),
    }
}

pub(crate) fn into_tuple<const N: usize>(t: Token) -> anyhow::Result<[Token; N]> {
    match t {
        Token::Tuple(ts) => ts.try_into().ok().context("bad size"),
        bad => anyhow::bail!("want tuple, got {bad:?}"),
    }
}

pub(crate) fn into_uint<I: TryFrom<ethabi::Uint>>(t: Token) -> anyhow::Result<I> {
    match t {
        Token::Uint(i) => i.try_into().ok().context("overflow"),
        bad => anyhow::bail!("want uint, got {bad:?}"),
    }
}

pub(crate) fn into_bool(t: Token) -> anyhow::Result<bool> {
    match t {
        Token::Bool(b) => Ok(b),
        bad => anyhow::bail!("want bool, got {bad:?}"),
    }
}

#[cfg(test)]
fn example(t: &ethabi::ParamType) -> Token {
    use ethabi::ParamType as T;
    match t {
        T::Address => Token::Address(ethabi::Address::default()),
        T::Bytes => Token::Bytes(ethabi::Bytes::default()),
        T::Int(_) => Token::Int(ethabi::Int::default()),
        T::Uint(_) => Token::Uint(ethabi::Uint::default()),
        T::Bool => Token::Bool(bool::default()),
        T::String => Token::String(String::default()),
        T::Array(t) => Token::Array(vec![example(t)]),
        T::FixedBytes(n) => Token::FixedBytes(vec![0; *n]),
        T::FixedArray(t, n) => Token::FixedArray(vec![example(t); *n]),
        T::Tuple(ts) => Token::Tuple(ts.iter().map(example).collect()),
    }
}

#[cfg(test)]
impl<F: Function> Call<F> {
    pub(crate) fn test(&self) -> anyhow::Result<()> {
        self.calldata().context("calldata()")?;
        F::decode_outputs(
            self.function()
                .outputs
                .iter()
                .map(|p| example(&p.kind))
                .collect(),
        )?;
        Ok(())
    }
}
