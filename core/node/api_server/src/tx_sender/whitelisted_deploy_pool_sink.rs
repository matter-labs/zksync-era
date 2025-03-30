use zksync_dal::transactions_dal::L2TxSubmissionResult;
use zksync_multivm::interface::{tracer::ValidationTraces, VmEvent};
use zksync_types::{h256_to_address, l2::L2Tx, CONTRACT_DEPLOYER_ADDRESS};

use crate::{
    execution_sandbox::SandboxExecutionOutput,
    tx_sender::{
        master_pool_sink::MasterPoolSink, shared_allow_list::SharedAllowList, tx_sink::TxSink,
        SubmitTxError,
    },
};

/// Wrapper that submits transactions to the mempool and enforces contract deployment allow-list
/// by analyzing the VM events for contract deployments.
#[derive(Debug)]
pub struct WhitelistedDeployPoolSink {
    master_pool_sink: MasterPoolSink,
    shared_allow_list: SharedAllowList,
}

impl WhitelistedDeployPoolSink {
    pub fn new(master_pool_sink: MasterPoolSink, shared_allow_list: SharedAllowList) -> Self {
        Self {
            master_pool_sink,
            shared_allow_list,
        }
    }
}

#[async_trait::async_trait]
impl TxSink for WhitelistedDeployPoolSink {
    async fn submit_tx(
        &self,
        tx: &L2Tx,
        execution_outputs: &SandboxExecutionOutput,
        validation_traces: ValidationTraces,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        // Enforce the deployment allowlist by scanning for ContractDeployed events.
        // Each such event should follow this format:
        //   event ContractDeployed(address indexed deployerAddress, bytes32 indexed bytecodeHash, address indexed contractAddress);
        // We extract the deployer address from topic[1] and verify it is whitelisted.

        let deployer_addresses = execution_outputs.events.iter().filter_map(|event| {
            let is_contract_deployed = event.address == CONTRACT_DEPLOYER_ADDRESS
                && event.indexed_topics.first() == Some(&VmEvent::DEPLOY_EVENT_SIGNATURE);
            if is_contract_deployed {
                event.indexed_topics.get(1).map(h256_to_address)
            } else {
                None
            }
        });

        for deployer_address in deployer_addresses {
            if !self
                .shared_allow_list
                .is_address_allowed(&deployer_address)
                .await
            {
                tracing::info!(
                    "Blocking contract deployment in tx {:?}: deployer_address {:?} not whitelisted",
                    tx.hash(),
                    deployer_address
                );
                return Err(SubmitTxError::DeployerNotInAllowList(deployer_address));
            }
        }

        // If all deployment events pass the allowlist check, forward the submission.
        self.master_pool_sink
            .submit_tx(tx, execution_outputs, validation_traces)
            .await
    }
}
