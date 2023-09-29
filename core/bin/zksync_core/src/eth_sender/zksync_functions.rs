use zksync_contracts::zksync_contract;
use zksync_types::ethabi::Function;

#[derive(Debug)]
pub(super) struct ZkSyncFunctions {
    pub commit_l1_batches: Function,
    pub prove_l1_batches: Function,
    pub execute_l1_batches: Function,
}

impl Default for ZkSyncFunctions {
    fn default() -> Self {
        let zksync_contract = zksync_contract();

        let commit_l1_batches = zksync_contract
            .functions
            .get("commitBlocks") // Named this way for legacy reasons
            .cloned()
            .expect("commitBlocks function not found")
            .pop()
            .expect("commitBlocks function entry not found");

        let prove_l1_batches = zksync_contract
            .functions
            .get("proveBlocks")
            .cloned()
            .expect("proveBlocks function not found")
            .pop()
            .expect("proveBlocks function entry not found");

        let execute_l1_batches = zksync_contract
            .functions
            .get("executeBlocks")
            .cloned()
            .expect("executeBlocks function not found")
            .pop()
            .expect("executeBlocks function entry not found");

        Self {
            commit_l1_batches,
            prove_l1_batches,
            execute_l1_batches,
        }
    }
}
