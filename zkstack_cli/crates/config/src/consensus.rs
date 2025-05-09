use serde::{Deserialize, Serialize};
use zksync_basic_types::L2ChainId;

use crate::raw::PatchedConfig;

#[derive(Debug, Serialize, Deserialize)]
pub struct Weighted {
    pub key: String,
    pub weight: u64,
}

impl Weighted {
    pub fn new(key: String, weight: u64) -> Self {
        Self { key, weight }
    }
}

/// Less strictly typed version of the consensus genesis specification config.
#[derive(Debug)]
pub struct ConsensusGenesisSpecs {
    pub chain_id: L2ChainId,
    pub validators: Vec<Weighted>,
    pub leader: String,
}

/// Mirrors key–address pair used in the consensus config.
#[derive(Debug, Serialize)]
pub struct KeyAndAddress {
    pub key: String,
    pub addr: String,
}

#[derive(Debug)]
#[must_use = "Must be `save()`d for changes to take effect"]
pub struct ConsensusConfigPatch(pub(crate) PatchedConfig);

impl ConsensusConfigPatch {
    pub fn set_static_outbound_peers(&mut self, peers: Vec<KeyAndAddress>) -> anyhow::Result<()> {
        self.0.insert_yaml("gossip_static_outbound", peers)
    }

    pub async fn save(self) -> anyhow::Result<()> {
        self.0.save().await
    }
}
