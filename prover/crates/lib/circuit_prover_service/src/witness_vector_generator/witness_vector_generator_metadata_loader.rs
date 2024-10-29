use async_trait::async_trait;
use zksync_types::protocol_version::ProtocolSemanticVersion;
use zksync_types::prover_dal::FriProverJobMetadata;

use zksync_prover_dal::{Connection, Prover, ProverDal};

#[async_trait]
pub trait WitnessVectorMetadataLoader: Sync + Send + 'static {
    async fn load_metadata(&self, connection: Connection<'_, Prover>) -> Option<FriProverJobMetadata>;
}

pub struct LightWitnessVectorMetadataLoader {
    pod_name: String,
    protocol_version: ProtocolSemanticVersion,
}

impl LightWitnessVectorMetadataLoader {
    pub fn new(pod_name: String, protocol_version: ProtocolSemanticVersion) -> Self {
        Self {
            pod_name,
            protocol_version,
        }
    }
}

#[async_trait]
impl WitnessVectorMetadataLoader for LightWitnessVectorMetadataLoader {
    async fn load_metadata(&self, mut connection: Connection<'_, Prover>) -> Option<FriProverJobMetadata> {
        connection
            .fri_prover_jobs_dal()
            .get_light_job(self.protocol_version, &self.pod_name)
            .await
    }
}

pub struct HeavyWitnessVectorMetadataLoader {
    pod_name: String,
    protocol_version: ProtocolSemanticVersion,
}

impl HeavyWitnessVectorMetadataLoader {
    pub fn new(pod_name: String, protocol_version: ProtocolSemanticVersion) -> Self {
        Self {
            pod_name,
            protocol_version,
        }
    }
}

#[async_trait]
impl WitnessVectorMetadataLoader for HeavyWitnessVectorMetadataLoader {
    async fn load_metadata(&self, mut connection: Connection<'_, Prover>) -> Option<FriProverJobMetadata> {
        let metadata = connection
            .fri_prover_jobs_dal()
            .get_heavy_job(self.protocol_version, &self.pod_name)
            .await;
        if metadata.is_some() {
            return metadata;
        }
        connection
            .fri_prover_jobs_dal()
            .get_light_job(self.protocol_version, &self.pod_name)
            .await
    }
}