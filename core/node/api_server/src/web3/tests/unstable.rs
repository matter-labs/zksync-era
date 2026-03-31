//! Tests for the `unstable` Web3 namespace.

use zksync_types::tee_types::TeeType;
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
        let tee_type = TeeType::Sgx;
        let proof = client.airbender_proofs(batch_no, Some(tee_type)).await?;

        assert!(proof.is_empty());

        let pubkey = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let mut storage = pool.connection().await.unwrap();
        let mut airbender_proof_generation_dal = storage.airbender_proof_generation_dal();
        airbender_proof_generation_dal
            .insert_airbender_proof_generation_job(batch_no, tee_type)
            .await?;

        let signature = vec![0, 1, 2, 3, 4];
        let proof_vec = vec![5, 6, 7, 8, 9];
        airbender_proof_generation_dal
            .save_proof_artifacts_metadata(batch_no, tee_type, &pubkey, &signature, &proof_vec)
            .await?;

        let proofs = client.airbender_proofs(batch_no, Some(tee_type)).await?;
        assert!(proofs.len() == 1);
        let proof = &proofs[0];
        assert!(proof.l1_batch_number == batch_no);
        assert!(proof.tee_type == Some(tee_type));
        assert!(proof.pubkey.as_ref() == Some(&pubkey));
        assert!(proof.signature.as_ref() == Some(&signature));
        assert!(proof.proof.as_ref() == Some(&proof_vec));

        Ok(())
    }
}

#[tokio::test]
async fn get_airbender_proofs() {
    test_http_server(GetAirbenderProofsTest::new()).await;
}
