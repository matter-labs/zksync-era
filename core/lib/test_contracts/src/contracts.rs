//! Test contracts.

use ethabi::Token;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use zksync_types::{Execute, H256, U256};

/// The structure of produced modules is as follows:
///
/// - Each dir in `/contracts` translates into a module with the same name (just with `-` chars replaced with `_`).
/// - Each contract in all files in this dir produces a `RawContract` constant with the same name as the contract.
mod raw {
    #![allow(unused, non_upper_case_globals)]
    include!(concat!(env!("OUT_DIR"), "/raw_contracts.rs"));
}

mod raw_evm {
    #![allow(unused, non_upper_case_globals)]
    include!(concat!(env!("OUT_DIR"), "/raw_evm_contracts.rs"));
}

/// Raw EraVM contract produced by the build script.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RawContract {
    pub abi: &'static str,
    pub bytecode: &'static [u8],
}

/// Raw EVM contract produced by the build script.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RawEvmContract {
    pub abi: &'static str,
    pub init_bytecode: &'static [u8],
    pub deployed_bytecode: &'static [u8],
}

/// Test contract consisting of deployable EraVM bytecode and Web3 ABI.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TestContract {
    /// Web3 ABI of this contract.
    pub abi: ethabi::Contract,
    /// EraVM bytecode of this contract.
    pub bytecode: &'static [u8],
    /// Contract dependencies (i.e., potential factory deps to be included in the contract deployment / transactions).
    pub dependencies: Vec<TestContract>,
}

impl TestContract {
    fn new(raw: RawContract) -> Self {
        let abi = serde_json::from_str(raw.abi).expect("failed parsing contract ABI");
        Self {
            abi,
            bytecode: raw.bytecode,
            dependencies: vec![],
        }
    }

    pub fn bridge_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| {
            let mut contract = TestContract::new(raw::bridge_test::LegacySharedBridgeTest);
            contract.dependencies = vec![
                TestContract::new(raw::contract_libs::L2SharedBridgeLegacy),
                TestContract::new(raw::contract_libs::BridgedStandardERC20),
                TestContract::new(raw::contract_libs::ProxyAdmin),
                TestContract::new(raw::contract_libs::TransparentUpgradeableProxy),
                TestContract::new(raw::contract_libs::UpgradeableBeacon),
                TestContract::new(raw::bridge_test::L2StandardERC20V25),
                TestContract::new(raw::bridge_test::L2SharedBridgeV25),
            ];
            contract
        });
        &CONTRACT
    }

    /// Returns a contract used to test complex system contract upgrades.
    pub fn complex_upgrade() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::complex_upgrade::ComplexUpgrade));
        &CONTRACT
    }

    /// Returns a contract used to test context methods.
    pub fn context_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::context::Context));
        &CONTRACT
    }

    /// Returns a simple counter contract.
    pub fn counter() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::counter::Counter));
        &CONTRACT
    }

    /// Returns a counter factory contract.
    pub fn counter_factory() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| {
            let mut contract = TestContract::new(raw::counter::CounterFactory);
            contract.dependencies = vec![TestContract::new(raw::counter::Counter)];
            contract
        });
        &CONTRACT
    }

    /// Returns a contract used in load testing that emulates various kinds of expensive operations
    /// (storage reads / writes, hashing, recursion via far calls etc.).
    pub fn load_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| {
            let mut contract = TestContract::new(raw::loadnext::LoadnextContract);
            contract.dependencies = vec![TestContract::new(raw::loadnext::Foo)];
            contract
        });
        &CONTRACT
    }

    /// Returns a contract with expensive storage operations.
    pub fn expensive() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::expensive::Expensive));
        &CONTRACT
    }

    pub fn failed_call() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::failed_call::FailedCall));
        &CONTRACT
    }

    /// Returns a contract with an infinite loop (useful for testing out-of-gas reverts).
    pub fn infinite_loop() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::infinite::InfiniteLoop));
        &CONTRACT
    }

    pub fn permissive_account() -> &'static Self {
        static CONTRACT: Lazy<TestContract> = Lazy::new(|| {
            let mut contract = TestContract::new(raw::custom_account::PermissiveAccount);
            contract.dependencies = vec![TestContract::new(
                raw::custom_account::PermissiveAccountDeployedContract,
            )];
            contract
        });
        &CONTRACT
    }

    /// Returns a custom account with multiple owners.
    pub fn many_owners() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::custom_account::ManyOwnersCustomAccount));
        &CONTRACT
    }

    /// Returns a contract testing `msg.sender` value.
    pub fn msg_sender_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::complex_upgrade::MsgSenderTest));
        &CONTRACT
    }

    pub fn nonce_holder() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::custom_account::NonceHolderTest));
        &CONTRACT
    }

    pub fn validation_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::custom_account::ValidationRuleBreaker));
        &CONTRACT
    }

    pub fn validation_test_mock_token() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::custom_account::MockToken));
        &CONTRACT
    }

    /// Returns a contract testing precompiles.
    pub fn precompiles_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::precompiles::Precompiles));
        &CONTRACT
    }

    /// Returns a contract proxying calls to a [counter](Self::counter()).
    pub fn proxy_counter() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::counter::ProxyCounter));
        &CONTRACT
    }

    /// Returns a reentrant recipient for transfers.
    pub fn reentrant_recipient() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::transfer::ReentrantRecipient));
        &CONTRACT
    }

    /// Returns a contract testing reverts.
    pub fn reverts_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::error::SimpleRequire));
        &CONTRACT
    }

    /// Returns a simple fungible token contract.
    pub fn simple_transfer() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::simple_transfer::SimpleTransfer));
        &CONTRACT
    }

    /// Returns a contract testing storage operations.
    pub fn storage_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::storage::StorageTester));
        &CONTRACT
    }

    /// Returns a contract for testing base token transfers.
    pub fn transfer_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::transfer::TransferTest));
        &CONTRACT
    }

    /// Returns a test recipient for the [transfer test](Self::transfer_test()) contract.
    pub fn transfer_recipient() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::transfer::Recipient));
        &CONTRACT
    }

    /// Returns a test ERC20 token implementation.
    pub fn test_erc20() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::transfer::TestERC20));
        &CONTRACT
    }

    /// Returns a mock version of `ContractDeployer`.
    pub fn mock_deployer() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::mock_evm::MockContractDeployer));
        &CONTRACT
    }

    /// Returns a mock version of `KnownCodeStorage`.
    pub fn mock_known_code_storage() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::mock_evm::MockKnownCodeStorage));
        &CONTRACT
    }

    /// Returns a mock EVM emulator.
    pub fn mock_evm_emulator() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::mock_evm::MockEvmEmulator));
        &CONTRACT
    }

    /// Contract testing recursive calls.
    pub fn recursive_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::mock_evm::NativeRecursiveContract));
        &CONTRACT
    }

    /// Contract implementing incrementing operations. Used to test static / delegate calls.
    pub fn increment_test() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::mock_evm::IncrementingContract));
        &CONTRACT
    }

    pub fn eravm_tester() -> &'static Self {
        static CONTRACT: Lazy<TestContract> =
            Lazy::new(|| TestContract::new(raw::mock_evm::EraVmTester));
        &CONTRACT
    }

    /// Returns all factory deps for this contract deployment (excluding its own bytecode).
    pub fn factory_deps(&self) -> Vec<Vec<u8>> {
        let mut deps = vec![];
        self.insert_factory_deps(&mut deps);
        deps
    }

    fn insert_factory_deps(&self, dest: &mut Vec<Vec<u8>>) {
        for deployed in &self.dependencies {
            dest.push(deployed.bytecode.to_vec());
            deployed.insert_factory_deps(dest);
        }
    }

    /// Generates the `Execute` payload for deploying this contract with zero salt.
    pub fn deploy_payload(&self, args: &[Token]) -> Execute {
        self.deploy_payload_with_salt(H256::zero(), args)
    }

    /// Generates the `Execute` payload for deploying this contract with custom salt.
    pub fn deploy_payload_with_salt(&self, salt: H256, args: &[Token]) -> Execute {
        let mut execute = Execute::for_deploy(salt, self.bytecode.to_vec(), args);
        execute.factory_deps.extend(self.factory_deps());
        execute
    }

    /// Shortcut for accessing a function that panics if a function doesn't exist.
    pub fn function(&self, name: &str) -> &ethabi::Function {
        self.abi
            .function(name)
            .unwrap_or_else(|err| panic!("cannot access function `{name}`: {err}"))
    }
}

#[derive(Debug)]
pub struct TestEvmContract {
    /// Web3 ABI of this contract.
    pub abi: ethabi::Contract,
    /// Initialization / constructor bytecode of this contract.
    pub init_bytecode: &'static [u8],
    /// Deployed bytecode of this contract.
    pub deployed_bytecode: &'static [u8],
}

impl TestEvmContract {
    fn new(raw: RawEvmContract) -> Self {
        let abi = serde_json::from_str(raw.abi).expect("failed parsing contract ABI");
        Self {
            abi,
            init_bytecode: raw.init_bytecode,
            deployed_bytecode: raw.deployed_bytecode,
        }
    }

    pub fn counter() -> &'static Self {
        static CONTRACT: Lazy<TestEvmContract> =
            Lazy::new(|| TestEvmContract::new(raw_evm::evm::Counter));
        &CONTRACT
    }

    pub fn evm_tester() -> &'static Self {
        static CONTRACT: Lazy<TestEvmContract> =
            Lazy::new(|| TestEvmContract::new(raw_evm::evm::EvmEmulationTest));
        &CONTRACT
    }

    /// Shortcut for accessing a function that panics if a function doesn't exist.
    pub fn function(&self, name: &str) -> &ethabi::Function {
        self.abi
            .function(name)
            .unwrap_or_else(|err| panic!("cannot access function `{name}`: {err}"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadnextContractExecutionParams {
    pub reads: usize,
    pub initial_writes: usize,
    pub repeated_writes: usize,
    pub events: usize,
    pub hashes: usize,
    pub recursive_calls: usize,
    pub deploys: usize,
}

impl LoadnextContractExecutionParams {
    pub fn empty() -> Self {
        Self {
            reads: 0,
            initial_writes: 0,
            repeated_writes: 0,
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
            initial_writes: 10,
            repeated_writes: 10,
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
            Token::Uint(U256::from(self.initial_writes)),
            Token::Uint(U256::from(self.repeated_writes)),
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
