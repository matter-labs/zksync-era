use anyhow::Context as _;
use ethabi::{ParamType, Token};

use crate::consensus_l2_contracts::{
    Add, Call, CommitAttesterCommittee, ConsensusRegistry, Function, GetAttesterCommittee,
    Initialize, Owner,
};

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
        T::FixedBytes(n) => Token::FixedBytes(vec![0; *n]),
        T::FixedArray(t, n) => Token::FixedArray(vec![example(t); *n]),
        T::Tuple(ts) => Token::Tuple(ts.iter().map(example).collect()),
    }
}

impl<F: Function> Call<F> {
    fn test(&self) -> anyhow::Result<()> {
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

#[test]
fn test_consensus_registry_abi() {
    let c = ConsensusRegistry::load();
    c.call(GetAttesterCommittee).test().unwrap();
    c.call(Add::default()).test().unwrap();
    c.call(Initialize::default()).test().unwrap();
    c.call(CommitAttesterCommittee).test().unwrap();
    c.call(Owner).test().unwrap();
}
