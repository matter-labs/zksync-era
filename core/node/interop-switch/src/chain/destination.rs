use std::fmt;

use zksync_basic_types::L2ChainId;

#[async_trait::async_trait]
pub trait DestinationChain: 'static + fmt::Debug + Send + Sync {
    fn chain_id(&self) -> L2ChainId;
}

#[derive(Debug, Clone)]
pub struct LocalDestinationChain {
    chain_id: L2ChainId,
}

impl LocalDestinationChain {
    pub fn new(chain_id: L2ChainId) -> Self {
        Self { chain_id }
    }
}

impl DestinationChain for LocalDestinationChain {
    fn chain_id(&self) -> L2ChainId {
        self.chain_id
    }
}
