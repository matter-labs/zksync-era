use zksync_contracts::{hyperchain_contract, multicall_contract, verifier_contract};
use zksync_types::ethabi::{Contract, Function};

#[derive(Debug)]
pub(super) struct ZkSyncFunctions {
    pub(super) post_shared_bridge_commit: Function,
    pub(super) post_shared_bridge_prove: Function,
    pub(super) post_shared_bridge_execute: Function,
    pub(super) get_l2_bootloader_bytecode_hash: Function,
    pub(super) get_l2_default_account_bytecode_hash: Function,
    pub(super) get_verifier: Function,
    pub(super) get_verifier_params: Function,
    pub(super) get_protocol_version: Function,

    pub(super) verifier_contract: Contract,
    pub(super) verification_key_hash: Function,

    pub(super) multicall_contract: Contract,
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

impl Default for ZkSyncFunctions {
    fn default() -> Self {
        let zksync_contract = hyperchain_contract();
        let verifier_contract = verifier_contract();
        let multicall_contract = multicall_contract();

        let post_shared_bridge_commit = get_function(&zksync_contract, "commitBatchesSharedBridge");
        let post_shared_bridge_prove = get_function(&zksync_contract, "proveBatchesSharedBridge");
        let post_shared_bridge_execute =
            get_function(&zksync_contract, "executeBatchesSharedBridge");
        let get_l2_bootloader_bytecode_hash =
            get_function(&zksync_contract, "getL2BootloaderBytecodeHash");
        let get_l2_default_account_bytecode_hash =
            get_function(&zksync_contract, "getL2DefaultAccountBytecodeHash");
        let get_verifier = get_function(&zksync_contract, "getVerifier");
        let get_verifier_params = get_function(&zksync_contract, "getVerifierParams");
        let get_protocol_version = get_function(&zksync_contract, "getProtocolVersion");
        let aggregate3 = get_function(&multicall_contract, "aggregate3");
        let verification_key_hash = get_function(&verifier_contract, "verificationKeyHash");

        ZkSyncFunctions {
            post_shared_bridge_commit,
            post_shared_bridge_prove,
            post_shared_bridge_execute,
            get_l2_bootloader_bytecode_hash,
            get_l2_default_account_bytecode_hash,
            get_verifier,
            get_verifier_params,
            get_protocol_version,
            verifier_contract,
            verification_key_hash,
            multicall_contract,
            aggregate3,
        }
    }
}
