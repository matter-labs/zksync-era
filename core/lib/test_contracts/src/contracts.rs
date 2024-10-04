//! Test contracts.

use ethabi::Token;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use zksync_types::{Execute, H256, U256};

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

const COMPLEX_UPGRADE_CONTRACT: &str =
    include_contract!("complex-upgrade/complex-upgrade"::ComplexUpgrade);
const CONTEXT_CONTRACT: &str = include_contract!("context/context"::Context);
const COUNTER_CONTRACT: &str = include_contract!("counter/counter"::Counter);
const EXPENSIVE_CONTRACT: &str = include_contract!("expensive/expensive"::Expensive);
const FAILED_CALL_CONTRACT: &str = include_contract!("failed-call/failed_call"::FailedCall);
const INFINITE_LOOP_CONTRACT: &str = include_contract!("infinite/infinite"::InfiniteLoop);
const LOAD_TEST_CONTRACT: &str = include_contract!("loadnext/loadnext_contract"::LoadnextContract);
const LOAD_TEST_DEPLOYED_CONTRACT: &str = include_contract!("loadnext/loadnext_contract"::Foo);
const MANY_OWNERS_CONTRACT: &str =
    include_contract!("custom-account/many-owners-custom-account"::ManyOwnersCustomAccount);
const MSG_SENDER_TEST_CONTRACT: &str =
    include_contract!("complex-upgrade/msg-sender"::MsgSenderTest);
const NONCE_HOLDER_CONTRACT: &str =
    include_contract!("custom-account/nonce-holder-test"::NonceHolderTest);
const PRECOMPILES_CONTRACT: &str = include_contract!("precompiles/precompiles"::Precompiles);
const PROXY_COUNTER_CONTRACT: &str = include_contract!("counter/proxy_counter"::ProxyCounter);
const SIMPLE_TRANSFER_CONTRACT: &str =
    include_contract!("simple-transfer/simple-transfer"::SimpleTransfer);
const REENTRANT_RECIPIENT_CONTRACT: &str =
    include_contract!("transfer/transfer"::ReentrantRecipient);
const REVERTS_TEST_CONTRACT: &str = include_contract!("error/error"::SimpleRequire);
const STORAGE_TEST_CONTRACT: &str = include_contract!("storage/storage"::StorageTester);
const TRANSFER_RECIPIENT_CONTRACT: &str = include_contract!("transfer/transfer"::Recipient);
const TRANSFER_TEST_CONTRACT: &str = include_contract!("transfer/transfer"::TransferTest);

/// Test contract consisting of deployable EraVM bytecode and Web3 ABI.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TestContract {
    /// Web3 ABI of this contract.
    pub abi: ethabi::Contract,
    /// EraVM bytecode of this contract.
    pub bytecode: Vec<u8>,
    /// Contract dependencies (i.e., potential factory deps to be included in the contract deployment / transactions).
    pub dependencies: Vec<TestContract>,
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
            dependencies: vec![],
        }
    }

    /// Returns a contract used to test complex system contract upgrades.
    pub fn complex_upgrade() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(COMPLEX_UPGRADE_CONTRACT));
        &CONTRACT
    }

    /// Returns a contract used to test context methods.
    pub fn context_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(CONTEXT_CONTRACT));
        &CONTRACT
    }

    /// Returns a simple counter contract.
    pub fn counter() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(COUNTER_CONTRACT));
        &CONTRACT
    }

    /// Returns a contract used in load testing that emulates various kinds of expensive operations
    /// (storage reads / writes, hashing, recursion via far calls etc.).
    pub fn load_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| {
            let mut contract = TestContract::new(LOAD_TEST_CONTRACT);
            contract.dependencies = vec![TestContract::new(LOAD_TEST_DEPLOYED_CONTRACT)];
            contract
        });
        &CONTRACT
    }

    /// Returns a contract with expensive storage operations.
    pub fn expensive() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(EXPENSIVE_CONTRACT));
        &CONTRACT
    }

    pub fn failed_call() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(FAILED_CALL_CONTRACT));
        &CONTRACT
    }

    /// Returns a contract with an infinite loop (useful for testing out-of-gas reverts).
    pub fn infinite_loop() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(INFINITE_LOOP_CONTRACT));
        &CONTRACT
    }

    /// Returns a custom account with multiple owners.
    pub fn many_owners() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(MANY_OWNERS_CONTRACT));
        &CONTRACT
    }

    /// Returns a contract testing `msg.sender` value.
    pub fn msg_sender_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(MSG_SENDER_TEST_CONTRACT));
        &CONTRACT
    }

    pub fn nonce_holder() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(NONCE_HOLDER_CONTRACT));
        &CONTRACT
    }

    /// Returns a contract testing precompiles.
    pub fn precompiles_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| TestContract::new(PRECOMPILES_CONTRACT));
        &CONTRACT
    }

    /// Returns a contract proxying calls to a [counter](Self::counter()).
    pub fn proxy_counter() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(PROXY_COUNTER_CONTRACT));
        &CONTRACT
    }

    /// Returns a reentrant recipient for transfers.
    pub fn reentrant_recipient() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(REENTRANT_RECIPIENT_CONTRACT));
        &CONTRACT
    }

    /// Returns a contract testing reverts.
    pub fn reverts_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(REVERTS_TEST_CONTRACT));
        &CONTRACT
    }

    /// Returns a simple fungible token contract.
    pub fn simple_transfer() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(SIMPLE_TRANSFER_CONTRACT));
        &CONTRACT
    }

    /// Returns a contract testing storage operations.
    pub fn storage_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(STORAGE_TEST_CONTRACT));
        &CONTRACT
    }

    /// Returns a contract for testing base token transfers.
    pub fn transfer_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(TRANSFER_TEST_CONTRACT));
        &CONTRACT
    }

    /// Returns a test recipient for the [transfer test](Self::transfer_test()) contract.
    pub fn transfer_recipient() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(TRANSFER_RECIPIENT_CONTRACT));
        &CONTRACT
    }

    /// Returns all factory deps for this contract deployment (including its own bytecode).
    pub fn factory_deps(&self) -> Vec<Vec<u8>> {
        let mut deps = vec![];
        self.insert_factory_deps(&mut deps);
        deps
    }

    fn insert_factory_deps(&self, dest: &mut Vec<Vec<u8>>) {
        for deployed in &self.dependencies {
            dest.push(deployed.bytecode.clone());
            deployed.insert_factory_deps(dest);
        }
    }

    /// Generates the `Execute` payload for deploying this contract with zero salt.
    pub fn deploy_payload(&self, args: &[Token]) -> Execute {
        self.deploy_payload_with_salt(H256::zero(), args)
    }

    /// Generates the `Execute` payload for deploying this contract with custom salt.
    pub fn deploy_payload_with_salt(&self, salt: H256, args: &[Token]) -> Execute {
        let mut execute = Execute::for_deploy(salt, self.bytecode.clone(), args);
        execute.factory_deps.extend(self.factory_deps());
        execute
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
        TestContract::context_test()
            .abi
            .function("getBlockNumber")
            .unwrap();
    }
}
