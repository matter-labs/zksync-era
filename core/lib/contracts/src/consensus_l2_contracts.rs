use ethabi::{ethereum_types::{U256},Address,Bytes,Contract,Token};

use crate::{TestContract};

pub struct ConsensusRegistry(Contract);

impl ConsensusRegistry {
    const FILE: &str = "contracts/l2-contracts/artifacts-zk/contracts/ConsensusRegistry.sol/ConsensusRegistry.json";
    
    pub fn load() -> Self {
        Self(crate::load_contract(Self::FILE))
    }

    pub fn test_contract(&self) -> TestContract {
        TestContract {
            bytecode: crate::read_bytecode(Self::FILE),
            contract: self.0.clone(),
            factory_deps: vec![],
        }
    }
}

#[derive(Debug, Default)]
pub struct Attester {
    pub weight: U256,
    pub node_owner: Address,
    pub pub_key: Bytes,
}

/*
impl Detokenize for Attester {
    fn from_tokens(tokens: Vec<Token>) -> Result<Self, Error> {
        let mut tokens = tokens.into_iter();
        let next = |field| tokens.next().ok_or_else(|| Error::Other(format!("{field} missing")));
        Ok(Self {
            weight: U256::from_token(next("weight")?)?,
            node_owner: Address::from_token(next("node_owner")?)?,
            pub_key: Bytes::from_token(next("pub_key")?)?,
        })
    }
}*/
