use ethabi::Contract;

use crate::{load_contract, read_bytecode, TestContract};

const CONSENSUS_REGISTRY_CONTRACT_FILE: &str =
    "contracts/l2-contracts/artifacts-zk/contracts/ConsensusRegistry.sol/ConsensusRegistry.json";

pub fn load_consensus_registry_contract() -> Contract {
    load_contract(CONSENSUS_REGISTRY_CONTRACT_FILE)
}

pub fn load_consensus_registry_contract_in_test() -> TestContract {
    TestContract {
        bytecode: read_bytecode(CONSENSUS_REGISTRY_CONTRACT_FILE),
        contract: load_consensus_registry_contract(),
        factory_deps: vec![],
    }
}
