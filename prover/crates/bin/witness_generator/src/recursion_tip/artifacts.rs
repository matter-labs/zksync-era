use std::time::Instant;

use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_types::L1BatchNumber;

use crate::{
    recursion_tip::RecursionTipWitnessGenerator,
    traits::{ArtifactsManager, BlobUrls},
};

impl ArtifactsManager for RecursionTipWitnessGenerator {
    type InputMetadata = ();
    type InputArtifacts = ();
    type OutputArtifacts = ();
    type ArtifactsMetadata = ();

    async fn get_artifacts(
        metadata: &Self::Medatadata,
        object_store: &dyn ObjectStore,
    ) -> Self::InputArtifacts {
        todo!()
    }

    async fn save_artifacts(
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> BlobUrls {
        todo!()
    }

    async fn update_database(
        connection_pool: &ConnectionPool<Prover>,
        block_number: L1BatchNumber,
        started_at: Instant,
        blob_urls: BlobUrls,
        metadata: Self::ArtifactsMetadata,
    ) {
        todo!()
    }
}
