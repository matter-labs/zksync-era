use zksync_types::{L2ChainId, U256};

#[derive(Debug, Clone)]
pub struct NetNamespace {
    zksync_network_id: L2ChainId,
}

impl NetNamespace {
    pub fn new(zksync_network_id: L2ChainId) -> Self {
        Self { zksync_network_id }
    }

    pub fn version_impl(&self) -> String {
        self.zksync_network_id.as_u64().to_string()
    }

    pub fn peer_count_impl(&self) -> U256 {
        0.into()
    }

    pub fn is_listening_impl(&self) -> bool {
        false
    }
}
