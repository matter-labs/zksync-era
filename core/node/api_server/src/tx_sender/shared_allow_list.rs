use std::{collections::HashSet, sync::Arc};

use tokio::sync::RwLock;
use zksync_types::Address;

#[derive(Debug, Clone)]
pub struct SharedAllowList {
    inner: Arc<RwLock<HashSet<Address>>>,
}

impl SharedAllowList {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Returns the internal writer, useful for updating from node_framework
    pub fn writer(&self) -> Arc<RwLock<HashSet<Address>>> {
        Arc::clone(&self.inner)
    }

    /// Checks if the given address is in the allowlist
    pub async fn is_address_allowed(&self, address: &Address) -> bool {
        self.inner.read().await.contains(address)
    }
}
