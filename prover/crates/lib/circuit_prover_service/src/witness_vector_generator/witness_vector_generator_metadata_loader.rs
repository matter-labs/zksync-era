use async_trait::async_trait;
use zksync_prover_dal::{Connection, Prover, ProverDal};
use zksync_types::{protocol_version::ProtocolSemanticVersion, prover_dal::FriProverJobMetadata};

/// Trait responsible for describing the job loading interface.
/// This is necessary as multiple strategies are necessary for loading jobs (which require different implementations).
#[async_trait]
pub trait WitnessVectorMetadataLoader: Sync + Send + 'static {
    async fn load_metadata(
        &self,
        connection: Connection<'_, Prover>,
    ) -> Option<FriProverJobMetadata>;
}

/// Light job MetadataLoader.
///
/// Most jobs are light, apart from nodes. This loader will only pick non nodes jobs.
#[derive(Debug)]
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
    async fn load_metadata(
        &self,
        mut connection: Connection<'_, Prover>,
    ) -> Option<FriProverJobMetadata> {
        connection
            .fri_prover_jobs_dal()
            .get_light_job(self.protocol_version, &self.pod_name)
            .await
    }
}

/// Heavy job MetadataLoader.
///
/// Most jobs are light, apart from nodes. This loader will only prioritize node jobs.
/// If none are available, it will fall back to light jobs.
#[derive(Debug)]
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
    async fn load_metadata(
        &self,
        mut connection: Connection<'_, Prover>,
    ) -> Option<FriProverJobMetadata> {
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

/// Simple MetadataLoader.
///
/// This loader will pick any job available, regardless of its type.
#[derive(Debug)]
pub struct SimpleWitnessVectorMetadataLoader {
    pod_name: String,
    protocol_version: ProtocolSemanticVersion,
}

impl SimpleWitnessVectorMetadataLoader {
    pub fn new(pod_name: String, protocol_version: ProtocolSemanticVersion) -> Self {
        Self {
            pod_name,
            protocol_version,
        }
    }
}

#[async_trait]
impl WitnessVectorMetadataLoader for SimpleWitnessVectorMetadataLoader {
    async fn load_metadata(
        &self,
        mut connection: Connection<'_, Prover>,
    ) -> Option<FriProverJobMetadata> {
        connection
            .fri_prover_jobs_dal()
            .get_next_job(self.protocol_version, &self.pod_name)
            .await
    }
}
