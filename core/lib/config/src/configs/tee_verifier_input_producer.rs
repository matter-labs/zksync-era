use serde::Deserialize;
use zksync_basic_types::L2ChainId;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TeeVerifierInputProducerConfig {
    pub l2_chain_id: L2ChainId,
    pub max_attempts: u32,
}
