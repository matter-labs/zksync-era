//! Tests for KZG commitments.

use kzg::{
    boojum::pairing::{bls12_381::G1Compressed, EncodedPoint},
    verify_kzg_proof, verify_proof_poly,
    zkevm_circuits::eip_4844::ethereum_4844_data_into_zksync_pubdata,
};
use serde::{Deserialize, Serialize};

use super::*;

const KZG_TEST_JSON: &str = include_str!("kzg_test_0.json");

#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize)]
struct ExpectedOutputs {
    #[serde_as(as = "serde_with::hex::Hex")]
    versioned_hash: Vec<u8>,
    #[serde_as(as = "serde_with::hex::Hex")]
    kzg_commitment: Vec<u8>,
    #[serde_as(as = "serde_with::hex::Hex")]
    opening_point: Vec<u8>,
    #[serde_as(as = "serde_with::hex::Hex")]
    opening_value: Vec<u8>,
    #[serde_as(as = "serde_with::hex::Hex")]
    opening_proof: Vec<u8>,
    #[serde_as(as = "serde_with::hex::Hex")]
    blob_proof: Vec<u8>,
    #[serde_as(as = "serde_with::hex::Hex")]
    pubdata_commitment: Vec<u8>,
}

#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize)]
struct KzgTest {
    #[serde_as(as = "serde_with::hex::Hex")]
    pubdata: Vec<u8>,
    expected_outputs: ExpectedOutputs,
}

/// Copy of function from https://github.com/matter-labs/era-zkevm_test_harness/blob/99956050a7705e26e0e5aa0729348896a27846c7/src/kzg/mod.rs#L339
fn u8_repr_to_fr(bytes: &[u8]) -> Fr {
    assert_eq!(bytes.len(), 32);
    let mut ret = [0u64; 4];

    for (i, chunk) in bytes.chunks(8).enumerate() {
        let mut repr = [0u8; 8];
        repr.copy_from_slice(chunk);
        ret[3 - i] = u64::from_be_bytes(repr);
    }

    Fr::from_repr(FrRepr(ret)).unwrap()
}

fn bytes_to_g1(data: &[u8]) -> G1Affine {
    let mut compressed = G1Compressed::empty();
    let v = compressed.as_mut();
    v.copy_from_slice(data);
    compressed.into_affine().unwrap()
}

#[test]
fn kzg_test() {
    let kzg_test: KzgTest = serde_json::from_str(KZG_TEST_JSON).unwrap();
    let kzg_info = KzgInfo::new(&kzg_test.pubdata);

    // Verify all the fields were correctly computed
    assert_eq!(
        hex::encode(kzg_info.kzg_commitment),
        hex::encode(kzg_test.expected_outputs.kzg_commitment)
    );
    assert_eq!(
        hex::encode(kzg_info.opening_point),
        hex::encode(kzg_test.expected_outputs.opening_point)
    );
    assert_eq!(
        hex::encode(kzg_info.opening_value),
        hex::encode(kzg_test.expected_outputs.opening_value)
    );
    assert_eq!(
        hex::encode(kzg_info.opening_proof),
        hex::encode(kzg_test.expected_outputs.opening_proof)
    );
    assert_eq!(
        hex::encode(kzg_info.versioned_hash),
        hex::encode(kzg_test.expected_outputs.versioned_hash)
    );
    assert_eq!(
        hex::encode(kzg_info.blob_proof),
        hex::encode(kzg_test.expected_outputs.blob_proof)
    );

    // Verify data we need for blob commitment on L1 returns the correct data
    assert_eq!(
        hex::encode(kzg_info.to_pubdata_commitment()),
        hex::encode(kzg_test.expected_outputs.pubdata_commitment)
    );

    // Verify that the blob, commitment, and proofs are all valid
    let blob = ethereum_4844_data_into_zksync_pubdata(&kzg_info.blob);
    let mut poly = zksync_pubdata_into_monomial_form_poly(&blob);
    fft(&mut poly);
    bitreverse(&mut poly);

    let commitment = bytes_to_g1(&kzg_info.kzg_commitment);
    let blob_proof = bytes_to_g1(&kzg_info.blob_proof);

    let valid_blob_proof = verify_proof_poly(&KZG_SETTINGS, &poly, &commitment, &blob_proof);
    assert!(valid_blob_proof);

    let opening_point = u8_repr_to_fr(&kzg_info.opening_point);
    let opening_value = u8_repr_to_fr(&kzg_info.opening_value);
    let opening_proof = bytes_to_g1(&kzg_info.opening_proof);

    let valid_opening_proof = verify_kzg_proof(
        &KZG_SETTINGS,
        &commitment,
        &opening_point,
        &opening_value,
        &opening_proof,
    );
    assert!(valid_opening_proof);
}

#[test]
fn bytes_test() {
    let kzg_test: KzgTest = serde_json::from_str(KZG_TEST_JSON).unwrap();

    let kzg_info = KzgInfo::new(&kzg_test.pubdata);
    let encoded_info = kzg_info.to_bytes();
    assert_eq!(KzgInfo::SERIALIZED_SIZE, encoded_info.len());

    let decoded_kzg_info = KzgInfo::from_slice(&encoded_info);
    assert_eq!(kzg_info, decoded_kzg_info);
}
