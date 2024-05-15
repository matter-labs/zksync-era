use serde::Deserialize;
use zksync_basic_types::url::SensitiveUrl;

use crate::configs::consensus::ConsensusSecrets;

#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseSecrets {
    server_url: Option<SensitiveUrl>,
    prover_url: Option<SensitiveUrl>,
    server_replica_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct L1Secrets {
    l1_rpc_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Secrets {
    pub consensus: Option<ConsensusSecrets>,
    pub database: Option<DatabaseSecrets>,
    pub l1: Option<L1Secrets>,
}
