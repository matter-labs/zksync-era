//! Test contracts.

use ethabi::Token;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use zksync_types::U256;

macro_rules! include_contract {
    ($file_name:tt :: $contract_name:tt) => {
        include_str!(concat!(
            env!("OUT_DIR"),
            "/artifacts-zk/contracts/",
            $file_name,
            ".sol/",
            stringify!($contract_name),
            ".json"
        ))
    };
}

const CONTEXT_CONTRACT: &str = include_contract!("context/context"::Context);
const COUNTER_CONTRACT: &str = include_contract!("counter/counter"::Counter);
const EXPENSIVE_CONTRACT: &str = include_contract!("expensive/expensive"::Expensive);
const FAILED_CALL_CONTRACT: &str = include_contract!("failed-call/failed_call"::FailedCall);
const INFINITE_LOOP_CONTRACT: &str = include_contract!("infinite/infinite"::InfiniteLoop);
const LOAD_TEST_CONTRACT: &str = include_contract!("loadnext/loadnext_contract"::LoadnextContract);
const LOAD_TEST_DEPLOYED_CONTRACT: &str = include_contract!("loadnext/loadnext_contract"::Foo);
const PRECOMPILES_CONTRACT: &str = include_contract!("precompiles/precompiles"::Precompiles);
const STORAGE_TEST_CONTRACT: &str = include_contract!("storage/storage"::StorageTester);

#[derive(Debug, Clone)]
pub struct TestContract {
    pub abi: ethabi::Contract,
    pub bytecode: Vec<u8>,
    pub deployed: Vec<TestContract>,
}

impl TestContract {
    fn new(full_abi: &str) -> Self {
        let mut raw: serde_json::Value =
            serde_json::from_str(full_abi).expect("failed reading contract ABI");
        let raw = raw.as_object_mut().expect("contract is not an object");
        let abi = raw.remove("abi").expect("contract doesn't contain ABI");
        let abi = serde_json::from_value(abi).expect("failed parsing contract ABI");
        let bytecode = raw
            .get("bytecode")
            .expect("contract doesn't contain bytecode")
            .as_str()
            .expect("bytecode is not a string");
        let bytecode = bytecode.strip_prefix("0x").unwrap_or(bytecode);
        let bytecode = hex::decode(bytecode).expect("invalid bytecode");
        Self {
            abi,
            bytecode,
            deployed: vec![],
        }
    }

    pub fn context() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(CONTEXT_CONTRACT));
        &CONTRACT
    }

    pub fn counter() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(COUNTER_CONTRACT));
        &CONTRACT
    }

    pub fn load_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| {
            let mut contract = TestContract::new(LOAD_TEST_CONTRACT);
            contract.deployed = vec![TestContract::new(LOAD_TEST_DEPLOYED_CONTRACT)];
            contract
        });
        &CONTRACT
    }

    pub fn expensive() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(EXPENSIVE_CONTRACT));
        &CONTRACT
    }

    pub fn failed_call() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(FAILED_CALL_CONTRACT));
        &CONTRACT
    }

    pub fn infinite_loop() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(INFINITE_LOOP_CONTRACT));
        &CONTRACT
    }

    pub fn precompiles() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(PRECOMPILES_CONTRACT));
        &CONTRACT
    }

    pub fn storage_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(STORAGE_TEST_CONTRACT));
        &CONTRACT
    }

    pub fn factory_deps(&self) -> Vec<Vec<u8>> {
        let mut deps = vec![];
        self.insert_factory_deps(&mut deps);
        deps
    }

    fn insert_factory_deps(&self, dest: &mut Vec<Vec<u8>>) {
        for deployed in &self.deployed {
            dest.push(deployed.bytecode.clone());
            deployed.insert_factory_deps(dest);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadnextContractExecutionParams {
    pub reads: usize,
    pub writes: usize,
    pub events: usize,
    pub hashes: usize,
    pub recursive_calls: usize,
    pub deploys: usize,
}

impl LoadnextContractExecutionParams {
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
    pub fn to_bytes(&self) -> Vec<u8> {
        let contract_function = TestContract::load_test().abi.function("execute").unwrap();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contracts_are_initialized_correctly() {
        TestContract::counter().abi.function("get").unwrap();
        TestContract::context()
            .abi
            .function("getBlockNumber")
            .unwrap();
    }
}
