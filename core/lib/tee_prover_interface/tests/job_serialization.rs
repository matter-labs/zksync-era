//! Integration tests for object store serialization of job objects.

use zksync_tee_prover_interface::{api::SubmitTeeProofRequest, outputs::L1BatchTeeProofForL1};
use zksync_types::tee_types::TeeType;

#[test]
fn test_tee_proof_request_serialization() {
    let tee_proof_str = r#"{
        "signature": "0001020304",
        "pubkey": "0506070809",
        "proof": "0A0B0C0D0E",
        "tee_type": "sgx"
    }"#;
    let tee_proof_result = serde_json::from_str::<SubmitTeeProofRequest>(tee_proof_str).unwrap();
    let tee_proof_expected = SubmitTeeProofRequest(Box::new(L1BatchTeeProofForL1 {
        signature: vec![0, 1, 2, 3, 4],
        pubkey: vec![5, 6, 7, 8, 9],
        proof: vec![10, 11, 12, 13, 14],
        tee_type: TeeType::Sgx,
    }));
    assert_eq!(tee_proof_result, tee_proof_expected);
}
