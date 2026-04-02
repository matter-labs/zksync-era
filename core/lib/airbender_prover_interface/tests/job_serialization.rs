//! Integration tests for object store serialization of job objects.

use zksync_airbender_prover_interface::api::SubmitAirbenderProofRequest;

#[test]
fn test_airbender_proof_request_serialization() {
    let airbender_proof_str = r#"{
        "l1_batch_number": 42,
        "proof": "0A0B0C0D0E"
    }"#;
    let airbender_proof_result =
        serde_json::from_str::<SubmitAirbenderProofRequest>(airbender_proof_str).unwrap();
    let airbender_proof_expected = SubmitAirbenderProofRequest {
        l1_batch_number: 42,
        proof: vec![10, 11, 12, 13, 14],
    };
    assert_eq!(airbender_proof_result, airbender_proof_expected);
}
