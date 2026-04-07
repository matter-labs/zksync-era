//! Integration tests for object store serialization of job objects.

use zksync_airbender_prover_interface::{
    api::SubmitAirbenderProofRequest, outputs::L1BatchAirbenderProofForL1,
};
use zksync_types::tee_types::TeeType;

#[test]
fn test_airbender_proof_request_serialization() {
    let airbender_proof_str = r#"{
        "signature": "0001020304",
        "pubkey": "0506070809",
        "proof": "0A0B0C0D0E",
        "tee_type": "sgx"
    }"#;
    let airbender_proof_result =
        serde_json::from_str::<SubmitAirbenderProofRequest>(airbender_proof_str).unwrap();
    let airbender_proof_expected =
        SubmitAirbenderProofRequest(Box::new(L1BatchAirbenderProofForL1 {
            signature: vec![0, 1, 2, 3, 4],
            pubkey: vec![5, 6, 7, 8, 9],
            proof: vec![10, 11, 12, 13, 14],
            tee_type: TeeType::Sgx,
        }));
    assert_eq!(airbender_proof_result, airbender_proof_expected);
}
