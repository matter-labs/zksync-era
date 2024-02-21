// TODO: Remove once integrated into batch commitment
#![allow(dead_code)]

use std::convert::TryInto;

use sha2::Sha256;
use sha3::{Digest, Keccak256};
use zkevm_test_harness_1_3_3::ff::{PrimeField, PrimeFieldRepr};
use zkevm_test_harness_1_4_1::{
    kzg::{compute_commitment, compute_proof, compute_proof_poly, KzgSettings},
    zkevm_circuits::{
        boojum::pairing::{
            bls12_381::{Fr, FrRepr, G1Affine},
            CurveAffine,
        },
        eip_4844::{
            bitreverse, fft,
            input::{BLOB_CHUNK_SIZE, ELEMENTS_PER_4844_BLOCK},
            zksync_pubdata_into_ethereum_4844_data, zksync_pubdata_into_monomial_form_poly,
        },
    },
};

const ZK_SYNC_BYTES_PER_BLOB: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;
const EIP_4844_BYTES_PER_BLOB: usize = 32 * ELEMENTS_PER_4844_BLOCK;

/// Packed pubdata commitments.
/// Format: opening point (16 bytes) || claimed value (32 bytes) || commitment (48 bytes)
///         || opening proof (48 bytes)) = 144 bytes
const BYTES_PER_PUBDATA_COMMITMENT: usize = 144;

const VERSIONED_HASH_VERSION_KZG: u8 = 0x01;

/// All the info needed for both the network transaction and by our L1 contracts. As part of the network transaction we
/// need to encode the sidecar which contains the: blob, `kzg` commitment, and the blob proof. The transaction payload
/// will utilize the versioned hash. The info needed for `commitBatches` is the `kzg` commitment, opening point,
/// opening value, and opening proof.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct KzgInfo {
    /// 4844 Compatible blob containing pubdata
    pub blob: [u8; EIP_4844_BYTES_PER_BLOB],
    /// KZG commitment to the blob
    pub kzg_commitment: [u8; 48],
    /// Point used by the point evaluation precompile
    pub opening_point: [u8; 32],
    /// Value retrieved by evaluation the `kzg` commitment at the `opening_point`
    pub opening_value: [u8; 32],
    /// Proof that opening the `kzg` commitment at the opening point yields the opening value
    pub opening_proof: [u8; 48],
    /// Hash of the `kzg` commitment where the first byte has been substituted for `VERSIONED_HASH_VERSION_KZG`
    pub versioned_hash: [u8; 32],
    /// Proof that the blob and `kzg` commitment represent the same data.
    pub blob_proof: [u8; 48],
}

/// Given a KZG commitment, calculate the versioned hash.
fn commitment_to_versioned_hash(kzg_commitment: G1Affine) -> [u8; 32] {
    let mut versioned_hash = [0u8; 32];

    let mut versioned_hash_bytes = Sha256::digest(kzg_commitment.into_compressed());
    versioned_hash_bytes[0] = VERSIONED_HASH_VERSION_KZG;

    versioned_hash.copy_from_slice(&versioned_hash_bytes);
    versioned_hash
}

/// Calculate the opening point for a given `linear_hash` and `versioned_hash`. We calculate
/// this point by hashing together the linear hash and versioned hash and only taking the last 16 bytes
fn compute_opening_point(linear_hash: [u8; 32], versioned_hash: [u8; 32]) -> u128 {
    let evaluation_point = &Keccak256::digest([linear_hash, versioned_hash].concat())[16..];

    u128::from_be_bytes(evaluation_point.try_into().expect("should have 16 bytes"))
}

/// Copies the specified number of bytes from the input into out returning the rest of the data
fn copy_n_bytes_return_rest<'a>(out: &'a mut [u8], input: &'a [u8], n: usize) -> &'a [u8] {
    let (bytes, data) = input.split_at(n);
    out.copy_from_slice(bytes);
    data
}

impl KzgInfo {
    /// Size of `KzgInfo` is equal to size(blob) + size(`kzg_commitment`) + size(bytes32) + size(bytes32)
    /// + size(`kzg_proof`) + size(bytes32) + size(`kzg_proof`)
    /// Here we use the size of the blob expected for 4844 (4096 elements * 32 bytes per element) and not
    /// `BYTES_PER_BLOB_ZK_SYNC` which is (4096 elements * 31 bytes per element)
    /// The zksync interpretation of the blob uses 31 byte fields so we can ensure they fit into a field element.
    const SERIALIZED_SIZE: usize = EIP_4844_BYTES_PER_BLOB + 48 + 32 + 32 + 48 + 32 + 48;

    /// Returns the bytes necessary for pubdata commitment part of batch commitments when blobs are used.
    /// Return format: opening point (16 bytes) || claimed value (32 bytes) || commitment (48 bytes)
    ///                || opening proof (48 bytes))
    pub fn to_pubdata_commitment(&self) -> [u8; BYTES_PER_PUBDATA_COMMITMENT] {
        let mut res = [0u8; BYTES_PER_PUBDATA_COMMITMENT];
        // The crypto team/batch commitment expects the opening point to be 16 bytes
        let mut truncated_opening_point = [0u8; 16];
        truncated_opening_point.copy_from_slice(&self.opening_point.as_slice()[16..]);
        res[0..16].copy_from_slice(&truncated_opening_point);
        res[16..48].copy_from_slice(self.opening_value.as_slice());
        res[48..96].copy_from_slice(self.kzg_commitment.as_slice());
        res[96..144].copy_from_slice(self.opening_proof.as_slice());
        res
    }

    /// Deserializes `Self::SERIALIZED_SIZE` bytes into `KzgInfo` struct
    pub fn from_slice(data: &[u8]) -> Self {
        assert_eq!(data.len(), Self::SERIALIZED_SIZE);

        let mut blob = [0u8; EIP_4844_BYTES_PER_BLOB];
        let data = copy_n_bytes_return_rest(&mut blob, data, EIP_4844_BYTES_PER_BLOB);

        let mut kzg_commitment = [0u8; 48];
        let data = copy_n_bytes_return_rest(&mut kzg_commitment, data, 48);

        let mut opening_point = [0u8; 32];
        let data = copy_n_bytes_return_rest(&mut opening_point, data, 32);

        let mut opening_value = [0u8; 32];
        let data = copy_n_bytes_return_rest(&mut opening_value, data, 32);

        let mut opening_proof = [0u8; 48];
        let data = copy_n_bytes_return_rest(&mut opening_proof, data, 48);

        let mut versioned_hash = [0u8; 32];
        let data = copy_n_bytes_return_rest(&mut versioned_hash, data, 32);

        let mut blob_proof = [0u8; 48];
        let data = copy_n_bytes_return_rest(&mut blob_proof, data, 48);

        assert_eq!(data.len(), 0);

        Self {
            blob,
            kzg_commitment,
            opening_point,
            opening_value,
            opening_proof,
            versioned_hash,
            blob_proof,
        }
    }

    /// Converts `KzgInfo` struct into a byte array
    pub fn to_bytes(&self) -> [u8; Self::SERIALIZED_SIZE] {
        let mut res = [0u8; Self::SERIALIZED_SIZE];

        let mut ptr = 0;

        res[ptr..ptr + EIP_4844_BYTES_PER_BLOB].copy_from_slice(self.blob.as_slice());
        ptr += EIP_4844_BYTES_PER_BLOB;

        res[ptr..ptr + 48].copy_from_slice(self.kzg_commitment.as_slice());
        ptr += 48;

        res[ptr..ptr + 32].copy_from_slice(self.opening_point.as_slice());
        ptr += 32;

        res[ptr..ptr + 32].copy_from_slice(self.opening_value.as_slice());
        ptr += 32;

        res[ptr..ptr + 48].copy_from_slice(self.opening_proof.as_slice());
        ptr += 48;

        res[ptr..ptr + 32].copy_from_slice(self.versioned_hash.as_slice());
        ptr += 32;

        res[ptr..ptr + 48].copy_from_slice(self.blob_proof.as_slice());
        ptr += 48;

        assert_eq!(ptr, Self::SERIALIZED_SIZE);

        res
    }

    /// Construct all the KZG info we need for turning a piece of zksync pubdata into a 4844 blob.
    /// The information we need is:
    ///     1. zksync blob <- `pad_right`(pubdata)
    ///     2. linear hash <- hash(zksync blob)
    ///     3. 4844 blob <- `zksync_pubdata_into_ethereum_4844_data`(zksync blob)
    ///     4. `kzg` polynomial <- `zksync_pubdata_into_monomial_form_poly`(zksync blob)
    ///     5. 4844 `kzg` commitment <- `compute_commitment`(4844 blob)
    ///     6. versioned hash <- hash(4844 `kzg` commitment)
    ///     7. opening point <- keccak(linear hash || versioned hash)[16..]
    ///     8. opening value, opening proof <- `compute_kzg_proof`(4844)
    ///     9. blob proof <- `compute_proof_poly`(blob, 4844 `kzg` commitment)
    pub fn new(kzg_settings: &KzgSettings, pubdata: Vec<u8>) -> Self {
        assert!(pubdata.len() <= ZK_SYNC_BYTES_PER_BLOB);

        let mut zksync_blob = [0u8; ZK_SYNC_BYTES_PER_BLOB];
        zksync_blob[0..pubdata.len()].copy_from_slice(&pubdata);

        let linear_hash: [u8; 32] = Keccak256::digest(zksync_blob).into();

        // We need to convert pubdata into poly form and apply `fft/bitreverse` transformations
        let mut poly = zksync_pubdata_into_monomial_form_poly(&zksync_blob);
        fft(&mut poly);
        bitreverse(&mut poly);

        let kzg_commitment = compute_commitment(kzg_settings, &poly);
        let versioned_hash = commitment_to_versioned_hash(kzg_commitment);
        let opening_point = compute_opening_point(linear_hash, versioned_hash);
        let opening_point_repr = Fr::from_repr(FrRepr([
            opening_point as u64,
            (opening_point >> 64) as u64,
            0u64,
            0u64,
        ]))
        .expect("should have a valid field element from 16 bytes");

        let (opening_proof, opening_value) =
            compute_proof(kzg_settings, &poly, &opening_point_repr);

        let blob_proof = compute_proof_poly(kzg_settings, &poly, &kzg_commitment);

        let blob_bytes = zksync_pubdata_into_ethereum_4844_data(&zksync_blob);
        let mut blob = [0u8; EIP_4844_BYTES_PER_BLOB];
        blob.copy_from_slice(&blob_bytes);

        let mut commitment = [0u8; 48];
        commitment.copy_from_slice(kzg_commitment.into_compressed().as_ref());

        let mut challenge_point = [0u8; 32];
        challenge_point[16..].copy_from_slice(&opening_point.to_be_bytes());

        let mut challenge_value = [0u8; 32];
        opening_value
            .into_repr()
            .write_be(&mut challenge_value[..])
            .unwrap();

        let mut challenge_proof = [0u8; 48];
        challenge_proof.copy_from_slice(opening_proof.into_compressed().as_ref());

        let mut commitment_proof = [0u8; 48];
        commitment_proof.copy_from_slice(blob_proof.into_compressed().as_ref());

        Self {
            blob,
            kzg_commitment: commitment,
            opening_point: challenge_point,
            opening_value: challenge_value,
            opening_proof: challenge_proof,
            versioned_hash,
            blob_proof: commitment_proof,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_with::{self, serde_as};
    use zkevm_test_harness_1_4_1::{
        boojum::pairing::{
            bls12_381::{Fr, FrRepr, G1Affine, G1Compressed},
            EncodedPoint,
        },
        kzg::{verify_kzg_proof, verify_proof_poly, KzgSettings},
        zkevm_circuits::eip_4844::{
            bitreverse, ethereum_4844_data_into_zksync_pubdata, fft,
            zksync_pubdata_into_monomial_form_poly,
        },
    };

    use super::{KzgInfo, PrimeField};

    #[serde_as]
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

    #[serde_as]
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
        let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let path = std::path::Path::new(&zksync_home).join("etc/kzg_tests/kzg_test_0.json");
        let contents = std::fs::read_to_string(path).unwrap();
        let kzg_test: KzgTest = serde_json::from_str(&contents).unwrap();

        let path = std::path::Path::new(&zksync_home).join("trusted_setup.json");
        let kzg_settings = KzgSettings::new(path.to_str().unwrap());

        let kzg_info = KzgInfo::new(&kzg_settings, kzg_test.pubdata);

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

        let valid_blob_proof = verify_proof_poly(&kzg_settings, &poly, &commitment, &blob_proof);
        assert!(valid_blob_proof);

        let opening_point = u8_repr_to_fr(&kzg_info.opening_point);
        let opening_value = u8_repr_to_fr(&kzg_info.opening_value);
        let opening_proof = bytes_to_g1(&kzg_info.opening_proof);

        let valid_opening_proof = verify_kzg_proof(
            &kzg_settings,
            &commitment,
            &opening_point,
            &opening_value,
            &opening_proof,
        );
        assert!(valid_opening_proof);
    }

    #[test]
    fn bytes_test() {
        let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let path = std::path::Path::new(&zksync_home).join("etc/kzg_tests/kzg_test_0.json");
        let contents = std::fs::read_to_string(path).unwrap();
        let kzg_test: KzgTest = serde_json::from_str(&contents).unwrap();

        let path = std::path::Path::new(&zksync_home).join("trusted_setup.json");
        let kzg_settings = KzgSettings::new(path.to_str().unwrap());

        let kzg_info = KzgInfo::new(&kzg_settings, kzg_test.pubdata);

        let encoded_info = kzg_info.to_bytes();
        assert_eq!(KzgInfo::SERIALIZED_SIZE, encoded_info.len());

        let decoded_kzg_info = KzgInfo::from_slice(&encoded_info);

        assert_eq!(kzg_info, decoded_kzg_info);
    }
}
