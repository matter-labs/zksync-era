use crate::get_loadnext_contract;
use ethabi::ethereum_types::U256;
use ethabi::{Bytes, Token};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct LoadnextContractExecutionParams {
    pub reads: usize,
    pub writes: usize,
    pub events: usize,
    pub hashes: usize,
    pub recursive_calls: usize,
    pub deploys: usize,
}

impl LoadnextContractExecutionParams {
    pub fn from_env() -> Option<Self> {
        envy::prefixed("CONTRACT_EXECUTION_PARAMS_").from_env().ok()
    }

    pub fn empty() -> Self {
        Self {
            reads: 0,
            writes: 0,
            events: 0,
            hashes: 0,
            recursive_calls: 0,
            deploys: 0,
        }
    }
}

impl Default for LoadnextContractExecutionParams {
    fn default() -> Self {
        Self {
            reads: 10,
            writes: 10,
            events: 10,
            hashes: 10,
            recursive_calls: 1,
            deploys: 1,
        }
    }
}

impl LoadnextContractExecutionParams {
    pub fn to_bytes(&self) -> Bytes {
        let loadnext_contract = get_loadnext_contract();
        let contract_function = loadnext_contract.contract.function("execute").unwrap();

        let params = vec![
            Token::Uint(U256::from(self.reads)),
            Token::Uint(U256::from(self.writes)),
            Token::Uint(U256::from(self.hashes)),
            Token::Uint(U256::from(self.events)),
            Token::Uint(U256::from(self.recursive_calls)),
            Token::Uint(U256::from(self.deploys)),
        ];

        contract_function
            .encode_input(&params)
            .expect("failed to encode parameters")
    }
}
