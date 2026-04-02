//! Tests for the `unstable` Web3 namespace.

use zksync_web3_decl::namespaces::UnstableNamespaceClient;

use super::*;

#[derive(Debug)]
struct GetAirbenderProofTest {}

impl GetAirbenderProofTest {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl HttpTest for GetAirbenderProofTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let batch_no = L1BatchNumber(1337);
        let proof = client.airbender_proof(batch_no).await?;

        assert!(proof.is_none());

        let mut storage = pool.connection().await.unwrap();
        let mut airbender_proof_generation_dal = storage.airbender_proof_generation_dal();
        airbender_proof_generation_dal
            .insert_airbender_proof_generation_job(batch_no)
            .await?;

        let proof_blob_url = "l1_batch_airbender_proof_1337.bin";
        airbender_proof_generation_dal
            .save_proof_artifacts_metadata(batch_no, proof_blob_url)
            .await?;

        let proof = client.airbender_proof(batch_no).await?;
        let proof = proof.expect("proof should exist");
        assert!(proof.l1_batch_number == batch_no);
        // proof bytes are None because the test doesn't configure an object store
        assert!(proof.proof.is_none());

        Ok(())
    }
}

#[tokio::test]
async fn get_airbender_proof() {
    test_http_server(GetAirbenderProofTest::new()).await;
}
