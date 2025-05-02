use std::{collections::HashSet, sync::Arc};

use tokio::sync::RwLock;
use zksync_multivm::{interface::VmEvent, zk_evm_latest::ethereum_types::Address};
use zksync_types::{h256_to_address, CONTRACT_DEPLOYER_ADDRESS};

/// SharedAllowList is a thread-safe wrapper around a HashSet of whitelisted addresses
#[derive(Debug, Clone, Default)]
pub struct SharedAllowList {
    inner: Arc<RwLock<HashSet<Address>>>,
}

impl From<Vec<Address>> for SharedAllowList {
    fn from(addresses: Vec<Address>) -> Self {
        let inner = Arc::new(RwLock::new(HashSet::from_iter(addresses)));
        Self { inner }
    }
}

impl SharedAllowList {
    pub fn new(addresses: HashSet<Address>) -> Self {
        let inner = Arc::new(RwLock::new(addresses));
        Self { inner }
    }

    pub fn writer(&self) -> &Arc<RwLock<HashSet<Address>>> {
        &self.inner
    }

    pub async fn is_address_allowed(&self, address: &Address) -> bool {
        self.inner.read().await.contains(address)
    }
}

/// DeploymentTxFilter is responsible for filtering deployment transactions based on the addresses allowlist.
#[derive(Debug)]
pub struct DeploymentTxFilter {
    shared_allow_list: SharedAllowList,
}

impl DeploymentTxFilter {
    pub fn new(shared_allow_list: SharedAllowList) -> Self {
        Self { shared_allow_list }
    }

    pub async fn find_not_allowed_deployer(
        &self,
        initiator: Address,
        vm_events: &[VmEvent],
    ) -> Option<Address> {
        // Check if the transaction initiator is allowlisted
        if self.shared_allow_list.is_address_allowed(&initiator).await {
            // If initiator is allowlisted, permit all deployments in this transaction
            return None;
        }

        // If initiator is not allowlisted, enforce the deployment allowlist by scanning for ContractDeployed events.
        // Each such event should follow this format:
        //   event ContractDeployed(address indexed deployerAddress, bytes32 indexed bytecodeHash, address indexed contractAddress);
        // We extract the deployer address from topic[1] and verify it is whitelisted.

        let deployer_addresses = vm_events.iter().filter_map(|event| {
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
                    "Blocking contract deployment. deployer_address {:?} not whitelisted",
                    deployer_address
                );
                return Some(deployer_address);
            }
        }

        // All checks passed, deployment is allowed
        None
    }
}
