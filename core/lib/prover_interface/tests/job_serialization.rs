//! Integration tests for object store serialization of job objects.

use bellman::plonk::better_better_cs::proof::Proof;
use tokio::fs;
use zksync_object_store::StoredObject;
use zksync_prover_interface::{
    api::SubmitProofRequest,
    outputs::{L1BatchProofForL1, PlonkL1BatchProofForL1, TypedL1BatchProofForL1},
};
use zksync_types::{protocol_version::ProtocolSemanticVersion, ProtocolVersionId};

#[tokio::test]
async fn test_final_proof_deserialization_cbor() {
    let proof = fs::read("./tests/l1_batch_proof_1_0_27_0.cbor")
        .await
        .unwrap();

    let results: L1BatchProofForL1 = StoredObject::deserialize(proof).unwrap();

    let coords = match results.inner() {
        TypedL1BatchProofForL1::Fflonk(proof) => proof.aggregation_result_coords,
        TypedL1BatchProofForL1::Plonk(proof) => proof.aggregation_result_coords,
    };

    assert_eq!(coords[0][0], 7);
}

#[test]
fn test_proof_request_serialization() {
    let proof = SubmitProofRequest::Proof(Box::new(
        L1BatchProofForL1::new_plonk(PlonkL1BatchProofForL1 {
            aggregation_result_coords: [[0; 32]; 4],
            scheduler_proof: Proof::empty(),
            protocol_version: ProtocolSemanticVersion {
                minor: ProtocolVersionId::Version25,
                patch: 10.into(),
            },
        })
        .into(),
    ));
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
    let (SubmitProofRequest::Proof(decoded_obj), SubmitProofRequest::Proof(decoded_json)) =
        (decoded_obj, decoded_json);

    let decoded_obj: L1BatchProofForL1 = (*decoded_obj).into();
    let decoded_json: L1BatchProofForL1 = (*decoded_json).into();

    let obj_coords = decoded_obj.aggregation_result_coords();
    let json_coords = decoded_json.aggregation_result_coords();

    assert_eq!(obj_coords, json_coords);
}
