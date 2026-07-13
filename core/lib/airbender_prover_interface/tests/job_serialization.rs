//! Integration tests for object store serialization of job objects.

use zksync_airbender_prover_interface::api::SubmitAirbenderProofRequest;

#[test]
fn test_airbender_proof_request_serialization() {
    // `error` and `cycles_used` are omitted by older provers and default to `None`.
    let airbender_proof_str = r#"{
        "l1_batch_number": 42,
        "prover_id": "prover-1",
        "proof": "0A0B0C0D0E"
    }"#;
    let airbender_proof_result =
        serde_json::from_str::<SubmitAirbenderProofRequest>(airbender_proof_str).unwrap();
    let airbender_proof_expected = SubmitAirbenderProofRequest {
        l1_batch_number: 42,
        prover_id: "prover-1".to_string(),
        proof: Some(vec![10, 11, 12, 13, 14]),
        error: None,
        cycles_used: None,
    };
    assert_eq!(airbender_proof_result, airbender_proof_expected);
}

#[test]
fn test_airbender_proof_request_with_cycles_serialization() {
    // A prover that measured the batch's guest cycles reports them alongside the proof.
    let airbender_proof_str = r#"{
        "l1_batch_number": 42,
        "prover_id": "prover-1",
        "proof": "0A0B0C0D0E",
        "cycles_used": 123456789
    }"#;
    let airbender_proof_result =
        serde_json::from_str::<SubmitAirbenderProofRequest>(airbender_proof_str).unwrap();
    let airbender_proof_expected = SubmitAirbenderProofRequest {
        l1_batch_number: 42,
        prover_id: "prover-1".to_string(),
        proof: Some(vec![10, 11, 12, 13, 14]),
        error: None,
        cycles_used: Some(123_456_789),
    };
    assert_eq!(airbender_proof_result, airbender_proof_expected);
}

#[test]
fn test_airbender_proof_failure_request_serialization() {
    // A failure submission carries `error` and omits `proof`.
    let airbender_proof_str = r#"{
        "l1_batch_number": 42,
        "prover_id": "prover-1",
        "error": "prover ran out of memory"
    }"#;
    let airbender_proof_result =
        serde_json::from_str::<SubmitAirbenderProofRequest>(airbender_proof_str).unwrap();
    let airbender_proof_expected = SubmitAirbenderProofRequest {
        l1_batch_number: 42,
        prover_id: "prover-1".to_string(),
        proof: None,
        error: Some("prover ran out of memory".to_string()),
        cycles_used: None,
    };
    assert_eq!(airbender_proof_result, airbender_proof_expected);
}
