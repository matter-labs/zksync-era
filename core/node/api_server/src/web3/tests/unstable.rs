//! Tests for the `unstable` Web3 namespace.

use zksync_types::tee_types::TeeType;
use zksync_web3_decl::namespaces::UnstableNamespaceClient;

use super::*;

#[derive(Debug)]
struct GetTeeProofsTest;

impl TestInit for GetTeeProofsTest {}

#[async_trait]
impl HttpTest for GetTeeProofsTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let batch_no = L1BatchNumber(1337);
        let tee_type = TeeType::Sgx;
        let proof = client.tee_proofs(batch_no, Some(tee_type)).await?;

        assert!(proof.is_empty());

        let pubkey = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let attestation = vec![0xC0, 0xFF, 0xEE];
        let mut storage = pool.connection().await.unwrap();
        let mut tee_proof_generation_dal = storage.tee_proof_generation_dal();
        tee_proof_generation_dal
            .save_attestation(&pubkey, &attestation)
            .await?;
        tee_proof_generation_dal
            .insert_tee_proof_generation_job(batch_no, tee_type)
            .await?;

        let signature = vec![0, 1, 2, 3, 4];
        let proof_vec = vec![5, 6, 7, 8, 9];
        tee_proof_generation_dal
            .save_proof_artifacts_metadata(batch_no, tee_type, &pubkey, &signature, &proof_vec)
            .await?;

        let proofs = client.tee_proofs(batch_no, Some(tee_type)).await?;
        assert!(proofs.len() == 1);
        let proof = &proofs[0];
        assert!(proof.l1_batch_number == batch_no);
        assert!(proof.tee_type == Some(tee_type));
        assert!(proof.pubkey.as_ref() == Some(&pubkey));
        assert!(proof.signature.as_ref() == Some(&signature));
        assert!(proof.proof.as_ref() == Some(&proof_vec));
        assert!(proof.attestation.as_ref() == Some(&attestation));

        Ok(())
    }
}

#[tokio::test]
async fn get_tee_proofs() {
    test_http_server(GetTeeProofsTest).await;
}
