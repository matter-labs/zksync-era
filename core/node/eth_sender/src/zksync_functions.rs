use zksync_contracts::{
    hyperchain_contract, multicall_contract, validator_timelock_contract, verifier_contract,
};
use zksync_types::ethabi::{Contract, Function};

#[derive(Debug)]
pub(super) struct ZkSyncFunctions {
    pub(super) pre_shared_bridge_commit: Function,
    pub(super) post_shared_bridge_commit: Option<Function>,
    pub(super) pre_shared_bridge_prove: Function,
    pub(super) post_shared_bridge_prove: Option<Function>,
    pub(super) pre_shared_bridge_execute: Function,
    pub(super) post_shared_bridge_execute: Option<Function>,
    pub(super) get_l2_bootloader_bytecode_hash: Function,
    pub(super) get_l2_default_account_bytecode_hash: Function,
    pub(super) get_verifier: Function,
    pub(super) get_verifier_params: Function,
    pub(super) get_protocol_version: Function,
    pub(super) is_batches_synced: Function,

    pub(super) verifier_contract: Contract,
    pub(super) verification_key_hash: Function,

    pub(super) multicall_contract: Contract,
    pub(super) validator_timelock_contract: Contract,
    pub(super) aggregate3: Function,
}

fn get_function(contract: &Contract, name: &str) -> Function {
    contract
        .functions
        .get(name)
        .cloned()
        .unwrap_or_else(|| panic!("{} function not found", name))
        .pop()
        .unwrap_or_else(|| panic!("{} function entry not found", name))
}

fn get_optional_function(contract: &Contract, name: &str) -> Option<Function> {
    contract
        .functions
        .get(name)
        .cloned()
        .map(|mut functions| functions.pop().unwrap())
}

impl Default for ZkSyncFunctions {
    fn default() -> Self {
        let zksync_contract = hyperchain_contract();
        let verifier_contract = verifier_contract();
        let multicall_contract = multicall_contract();
        let validator_timelock_contract = validator_timelock_contract();

        let pre_shared_bridge_commit = get_function(&zksync_contract, "commitBatches");
        let post_shared_bridge_commit =
            get_optional_function(&zksync_contract, "commitBatchesSharedBridge");
        let pre_shared_bridge_prove = get_function(&zksync_contract, "proveBatches");
        let post_shared_bridge_prove =
            get_optional_function(&zksync_contract, "proveBatchesSharedBridge");
        let pre_shared_bridge_execute = get_function(&zksync_contract, "executeBatches");
        let post_shared_bridge_execute =
            get_optional_function(&zksync_contract, "executeBatchesSharedBridge");
        let get_l2_bootloader_bytecode_hash =
            get_function(&zksync_contract, "getL2BootloaderBytecodeHash");
        let get_l2_default_account_bytecode_hash =
            get_function(&zksync_contract, "getL2DefaultAccountBytecodeHash");
        let get_verifier = get_function(&zksync_contract, "getVerifier");
        let get_verifier_params = get_function(&zksync_contract, "getVerifierParams");
        let get_protocol_version = get_function(&zksync_contract, "getProtocolVersion");
        let aggregate3 = get_function(&multicall_contract, "aggregate3");
        let is_batches_synced =
            get_function(&validator_timelock_contract, "isBatchesSyncedSharedBridge");
        let verification_key_hash = get_function(&verifier_contract, "verificationKeyHash");

        ZkSyncFunctions {
            pre_shared_bridge_commit,
            post_shared_bridge_commit,
            pre_shared_bridge_prove,
            post_shared_bridge_prove,
            pre_shared_bridge_execute,
            post_shared_bridge_execute,
            get_l2_bootloader_bytecode_hash,
            get_l2_default_account_bytecode_hash,
            get_verifier,
            get_verifier_params,
            get_protocol_version,
            is_batches_synced,
            verifier_contract,
            verification_key_hash,
            multicall_contract,
            validator_timelock_contract,
            aggregate3,
        }
    }
}
