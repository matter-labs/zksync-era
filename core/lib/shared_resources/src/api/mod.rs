//! Shared API types used in dependency injection.

use std::sync::Arc;

use tokio::sync::RwLock;
use zksync_node_framework::Resource;
use zksync_types::{api, Address};

pub use self::sync_state::{SyncState, SyncStateData};

mod sync_state;

/// Shared bridge addresses.
#[derive(Debug, Clone, Default)]
pub struct BridgeAddressesHandle(Arc<RwLock<Option<api::BridgeAddresses>>>);

impl BridgeAddressesHandle {
    pub fn new(addresses: api::BridgeAddresses) -> Self {
        Self(Arc::new(RwLock::new(Some(addresses))))
    }

    pub async fn update(&self, bridge_addresses: api::BridgeAddresses) {
        *self.0.write().await = Some(bridge_addresses);
    }

    pub async fn update_l1_shared_bridge(&self, l1_shared_bridge: Address) {
        if let Some(addresses) = &mut *self.0.write().await {
            addresses.l1_shared_default_bridge = Some(l1_shared_bridge);
        }
    }

    pub async fn update_l2_bridges(&self, l2_shared_bridge: Address) {
        if let Some(addresses) = &mut *self.0.write().await {
            addresses.l2_shared_default_bridge = Some(l2_shared_bridge);
            addresses.l2_erc20_default_bridge = Some(l2_shared_bridge);
        }
    }

    pub async fn read(&self) -> Option<api::BridgeAddresses> {
        self.0.read().await.clone()
    }
}

impl Resource for BridgeAddressesHandle {
    fn name() -> String {
        "api/bridge_addresses".into()
    }
}
