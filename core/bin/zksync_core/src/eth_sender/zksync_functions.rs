use zksync_contracts::zksync_contract;
use zksync_types::ethabi::Function;

#[derive(Debug)]
pub(super) struct ZkSyncFunctions {
    pub(super) commit_blocks: Function,
    pub(super) prove_blocks: Function,
    pub(super) execute_blocks: Function,
}

pub(super) fn get_zksync_functions() -> ZkSyncFunctions {
    let zksync_contract = zksync_contract();

    let commit_blocks = zksync_contract
        .functions
        .get("commitBlocks")
        .cloned()
        .expect("commitBlocks function not found")
        .pop()
        .expect("commitBlocks function entry not found");

    let prove_blocks = zksync_contract
        .functions
        .get("proveBlocks")
        .cloned()
        .expect("proveBlocks function not found")
        .pop()
        .expect("proveBlocks function entry not found");

    let execute_blocks = zksync_contract
        .functions
        .get("executeBlocks")
        .cloned()
        .expect("executeBlocks function not found")
        .pop()
        .expect("executeBlocks function entry not found");

    ZkSyncFunctions {
        commit_blocks,
        prove_blocks,
        execute_blocks,
    }
}
