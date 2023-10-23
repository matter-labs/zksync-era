use zksync_contracts::{
    multicall_contract, verifier_contract, zksync_contract, COMMIT_BLOCKS_FUNCTION,
    EXECUTE_BLOCKS_FUNCTION, PROVE_BLOCKS_FUNCTION,
};
use zksync_types::ethabi::{Contract, Function};

#[derive(Debug)]
pub(super) struct ZkSyncFunctions {
    pub(super) commit_blocks: Function,
    pub(super) commit_batches: Option<Function>,
    pub(super) prove_blocks: Function,
    pub(super) prove_batches: Option<Function>,
    pub(super) execute_blocks: Function,
    pub(super) execute_batches: Option<Function>,
    pub(super) get_l2_bootloader_bytecode_hash: Function,
    pub(super) get_l2_default_account_bytecode_hash: Function,
    pub(super) get_verifier: Function,
    pub(super) get_verifier_params: Function,
    pub(super) get_protocol_version: Function,

    pub(super) verifier_contract: Contract,
    pub(super) get_verification_key: Function,
    pub(super) verification_key_hash: Option<Function>,

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

fn get_optional_function(contract: &Contract, name: &str) -> Option<Function> {
    contract
        .functions
        .get(name)
        .cloned()
        .map(|mut functions| functions.pop().unwrap())
}

impl Default for ZkSyncFunctions {
    fn default() -> Self {
        let zksync_contract = zksync_contract();
        let verifier_contract = verifier_contract();
        let multicall_contract = multicall_contract();

        let commit_blocks = COMMIT_BLOCKS_FUNCTION.clone();
        let commit_batches = get_optional_function(&zksync_contract, "commitBatches");
        let prove_blocks = PROVE_BLOCKS_FUNCTION.clone();
        let prove_batches = get_optional_function(&zksync_contract, "proveBatches");
        let execute_blocks = EXECUTE_BLOCKS_FUNCTION.clone();
        let execute_batches = get_optional_function(&zksync_contract, "executeBatches");
        let get_l2_bootloader_bytecode_hash =
            get_function(&zksync_contract, "getL2BootloaderBytecodeHash");
        let get_l2_default_account_bytecode_hash =
            get_function(&zksync_contract, "getL2DefaultAccountBytecodeHash");
        let get_verifier = get_function(&zksync_contract, "getVerifier");
        let get_verifier_params = get_function(&zksync_contract, "getVerifierParams");
        let get_protocol_version = get_function(&zksync_contract, "getProtocolVersion");
        let get_verification_key = get_function(&verifier_contract, "get_verification_key");
        let aggregate3 = get_function(&multicall_contract, "aggregate3");
        let verification_key_hash =
            get_optional_function(&verifier_contract, "verificationKeyHash");

        ZkSyncFunctions {
            commit_blocks,
            commit_batches,
            prove_blocks,
            prove_batches,
            execute_blocks,
            execute_batches,
            get_l2_bootloader_bytecode_hash,
            get_l2_default_account_bytecode_hash,
            get_verifier,
            get_verifier_params,
            get_protocol_version,
            verifier_contract,
            get_verification_key,
            verification_key_hash,
            multicall_contract,
            aggregate3,
        }
    }
}
