//! Integration tests for object store serialization of job objects.

use circuit_sequencer_api_1_5_0::proof::FinalProof;
use tokio::fs;
use zksync_object_store::{Bucket, MockObjectStore};
use zksync_prover_interface::{
    api::{SubmitProofRequest, SubmitTeeProofRequest},
    inputs::{PrepareBasicCircuitsJob, StorageLogMetadata},
    outputs::{L1BatchProofForL1, L1BatchTeeProofForL1},
};
use zksync_types::{protocol_version::ProtocolSemanticVersion, L1BatchNumber, ProtocolVersionId};

/// Tests compatibility of the `PrepareBasicCircuitsJob` serialization to the previously used
/// one.
#[tokio::test]
async fn prepare_basic_circuits_job_serialization() {
    // The working dir for integration tests is set to the crate dir, so specifying relative paths
    // should be OK.
    let snapshot = fs::read("./tests/snapshots/prepare-basic-circuits-job-full.bin")
        .await
        .unwrap();
    let store = MockObjectStore::arc();
    store
        .put_raw(
            Bucket::WitnessInput,
            "merkel_tree_paths_1.bin",
            snapshot.clone(),
        )
        .await
        .unwrap();

    let job: PrepareBasicCircuitsJob = store.get(L1BatchNumber(1)).await.unwrap();

    let key = store.put(L1BatchNumber(2), &job).await.unwrap();
    let serialized_job = store.get_raw(Bucket::WitnessInput, &key).await.unwrap();
    assert_eq!(serialized_job, snapshot);
    assert_job_integrity(
        job.next_enumeration_index(),
        job.into_merkle_paths().collect(),
    );
}

fn assert_job_integrity(next_enumeration_index: u64, merkle_paths: Vec<StorageLogMetadata>) {
    assert_eq!(next_enumeration_index, 1);
    assert_eq!(merkle_paths.len(), 3);
    assert!(merkle_paths
        .iter()
        .all(|log| log.is_write && log.first_write));
    assert!(merkle_paths.iter().all(|log| log.merkle_paths.len() == 256));
}

/// Test that serialization works the same as with a tuple of the job fields.
#[tokio::test]
async fn prepare_basic_circuits_job_compatibility() {
    let snapshot = fs::read("./tests/snapshots/prepare-basic-circuits-job-full.bin")
        .await
        .unwrap();
    let job_tuple: (Vec<StorageLogMetadata>, u64) = bincode::deserialize(&snapshot).unwrap();

    let serialized = bincode::serialize(&job_tuple).unwrap();
    assert_eq!(serialized, snapshot);

    let job: PrepareBasicCircuitsJob = bincode::deserialize(&snapshot).unwrap();
    assert_eq!(job.next_enumeration_index(), job_tuple.1);
    let job_merkle_paths: Vec<_> = job.into_merkle_paths().collect();
    assert_eq!(job_merkle_paths, job_tuple.0);

    assert_job_integrity(job_tuple.1, job_tuple.0);
}

/// Simple test to check if we can successfully parse the proof.
#[tokio::test]
async fn test_final_proof_deserialization() {
    let proof = fs::read("./tests/l1_batch_proof_1_0_24_0.bin")
        .await
        .unwrap();

    let results: L1BatchProofForL1 = bincode::deserialize(&proof).unwrap();
    assert_eq!(results.aggregation_result_coords[0][0], 0);
}

#[test]
fn test_proof_request_serialization() {
    let proof = SubmitProofRequest::Proof(Box::new(L1BatchProofForL1 {
        aggregation_result_coords: [[0; 32]; 4],
        scheduler_proof: FinalProof::empty(),
        protocol_version: ProtocolSemanticVersion {
            minor: ProtocolVersionId::Version25,
            patch: 10.into(),
        },
    }));
    let encoded_obj = serde_json::to_string(&proof).unwrap();
    let encoded_json = r#"{
        "Proof": {
            "aggregation_result_coords": [
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                ],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                ],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                ],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                ]
            ],
            "scheduler_proof": {
                "n": 0,
                "inputs": [],
                "state_polys_commitments": [],
                "witness_polys_commitments": [],
                "copy_permutation_grand_product_commitment": {
                    "x": [ 0, 0, 0, 0 ],
                    "y": [ 1, 0, 0, 0 ],
                    "infinity": true
                },
                "lookup_s_poly_commitment": null,
                "lookup_grand_product_commitment": null,
                "quotient_poly_parts_commitments": [],
                "state_polys_openings_at_z": [],
                "state_polys_openings_at_dilations": [],
                "witness_polys_openings_at_z": [],
                "witness_polys_openings_at_dilations": [],
                "gate_setup_openings_at_z": [],
                "gate_selectors_openings_at_z": [],
                "copy_permutation_polys_openings_at_z": [],
                "copy_permutation_grand_product_opening_at_z_omega": [ 0, 0, 0, 0 ],
                "lookup_s_poly_opening_at_z_omega": null,
                "lookup_grand_product_opening_at_z_omega": null,
                "lookup_t_poly_opening_at_z": null,
                "lookup_t_poly_opening_at_z_omega": null,
                "lookup_selector_poly_opening_at_z": null,
                "lookup_table_type_poly_opening_at_z": null,
                "quotient_poly_opening_at_z": [ 0, 0, 0, 0 ],
                "linearization_poly_opening_at_z": [ 0, 0, 0, 0 ],
                "opening_proof_at_z": {
                    "x": [ 0, 0, 0, 0 ],
                    "y": [ 1, 0, 0, 0 ],
                    "infinity": true
                },
                "opening_proof_at_z_omega": {
                    "x": [ 0, 0, 0, 0 ],
                    "y": [ 1, 0, 0, 0 ],
                    "infinity": true
                }
            },
            "protocol_version": "0.25.10"
        }
    }"#;
    let decoded_obj: SubmitProofRequest = serde_json::from_str(&encoded_obj).unwrap();
    let decoded_json: SubmitProofRequest = serde_json::from_str(encoded_json).unwrap();
    match (decoded_obj, decoded_json) {
        (SubmitProofRequest::Proof(decoded_obj), SubmitProofRequest::Proof(decoded_json)) => {
            assert_eq!(
                decoded_obj.aggregation_result_coords,
                decoded_json.aggregation_result_coords
            );
        }
        _ => panic!("Either decoded_obj or decoded_json is not SubmitProofRequest::Proof"),
    }
}

#[test]
fn test_tee_proof_request_serialization() {
    let tee_proof_str = r#"{
        "signature": [ 0, 1, 2, 3, 4 ],
        "pubkey": [ 5, 6, 7, 8, 9 ],
        "proof": [ 10, 11, 12, 13, 14 ]
    }"#;
    let tee_proof_result = serde_json::from_str::<SubmitTeeProofRequest>(tee_proof_str).unwrap();
    let tee_proof_expected = SubmitTeeProofRequest(Box::new(L1BatchTeeProofForL1 {
        signature: vec![0, 1, 2, 3, 4],
        pubkey: vec![5, 6, 7, 8, 9],
        proof: vec![10, 11, 12, 13, 14],
    }));
    assert_eq!(tee_proof_result, tee_proof_expected);
}
