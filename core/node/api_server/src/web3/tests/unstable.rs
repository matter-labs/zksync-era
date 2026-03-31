//! Tests for the `unstable` Web3 namespace.

use zksync_web3_decl::namespaces::UnstableNamespaceClient;

use super::*;

#[derive(Debug)]
struct GetAirbenderProofsTest {}

impl GetAirbenderProofsTest {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl HttpTest for GetAirbenderProofsTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let batch_no = L1BatchNumber(1337);
        let proof = client.airbender_proofs(batch_no).await?;

        assert!(proof.is_empty());

        let mut storage = pool.connection().await.unwrap();
        let mut airbender_proof_generation_dal = storage.airbender_proof_generation_dal();
        airbender_proof_generation_dal
            .insert_airbender_proof_generation_job(batch_no)
            .await?;

        let proof_vec = vec![5, 6, 7, 8, 9];
        airbender_proof_generation_dal
            .save_proof_artifacts_metadata(batch_no, &proof_vec)
            .await?;

        let proofs = client.airbender_proofs(batch_no).await?;
        assert!(proofs.len() == 1);
        let proof = &proofs[0];
        assert!(proof.l1_batch_number == batch_no);
        assert!(proof.proof.as_ref() == Some(&proof_vec));

        Ok(())
    }
}

#[tokio::test]
async fn get_airbender_proofs() {
    test_http_server(GetAirbenderProofsTest::new()).await;
}
