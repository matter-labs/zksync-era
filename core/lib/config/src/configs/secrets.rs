use zksync_basic_types::url::SensitiveUrl;

use crate::configs::consensus::ConsensusSecrets;

#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseSecrets {
    pub server_url: Option<SensitiveUrl>,
    pub prover_url: Option<SensitiveUrl>,
    pub server_replica_url: Option<SensitiveUrl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct L1Secrets {
    pub l1_rpc_url: SensitiveUrl,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Secrets {
    pub consensus: Option<ConsensusSecrets>,
    pub database: Option<DatabaseSecrets>,
    pub l1: Option<L1Secrets>,
}
