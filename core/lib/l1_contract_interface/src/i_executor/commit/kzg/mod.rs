use std::convert::TryInto;

pub use kzg::KzgSettings;
use kzg::{
    compute_commitment, compute_proof, compute_proof_poly,
    zkevm_circuits::{
        boojum::pairing::{
            bls12_381::{Fr, FrRepr, G1Affine},
            ff::{PrimeField, PrimeFieldRepr},
            CurveAffine,
        },
        eip_4844::{
            bitreverse, fft,
            input::{BLOB_CHUNK_SIZE, ELEMENTS_PER_4844_BLOCK},
            zksync_pubdata_into_ethereum_4844_data, zksync_pubdata_into_monomial_form_poly,
        },
    },
};
use sha2::Sha256;
use sha3::{Digest, Keccak256};
use zksync_types::H256;

use self::trusted_setup::KZG_SETTINGS;

#[cfg(test)]
mod tests;
mod trusted_setup;

pub const ZK_SYNC_BYTES_PER_BLOB: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;
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
    ///                || opening proof (48 bytes)
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

    /// Computes the commitment to the blob needed as part of the batch commitment through the aux output
    /// Format is: Keccak(versioned hash || opening point || opening value)
    pub fn to_blob_commitment(&self) -> [u8; 32] {
        let mut commitment = [0u8; 32];
        let hash = &Keccak256::digest(
            [
                &self.versioned_hash,
                &self.opening_point[16..],
                &self.opening_value,
            ]
            .concat(),
        );
        commitment.copy_from_slice(hash);
        commitment
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
    ///     7. opening point <- keccak(linear hash || versioned hash)`[16..]`
    ///     8. opening value, opening proof <- `compute_kzg_proof`(4844)
    ///     9. blob proof <- `compute_proof_poly`(blob, 4844 `kzg` commitment)
    pub fn new(pubdata: &[u8]) -> Self {
        assert!(pubdata.len() <= ZK_SYNC_BYTES_PER_BLOB);

        let mut zksync_blob = [0u8; ZK_SYNC_BYTES_PER_BLOB];
        zksync_blob[0..pubdata.len()].copy_from_slice(pubdata);

        let linear_hash: [u8; 32] = Keccak256::digest(zksync_blob).into();

        // We need to convert pubdata into poly form and apply `fft/bitreverse` transformations
        let mut poly = zksync_pubdata_into_monomial_form_poly(&zksync_blob);
        fft(&mut poly);
        bitreverse(&mut poly);

        let kzg_commitment = compute_commitment(&KZG_SETTINGS, &poly);
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
            compute_proof(&KZG_SETTINGS, &poly, &opening_point_repr);

        let blob_proof = compute_proof_poly(&KZG_SETTINGS, &poly, &kzg_commitment);

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

pub fn pubdata_to_blob_commitments(num_blobs: usize, pubdata_input: &[u8]) -> Vec<H256> {
    assert!(
        pubdata_input.len() <= num_blobs * ZK_SYNC_BYTES_PER_BLOB,
        "Pubdata length exceeds size of blobs"
    );

    let mut blob_commitments = pubdata_input
        .chunks(ZK_SYNC_BYTES_PER_BLOB)
        .map(|blob| {
            let kzg_info = KzgInfo::new(blob);
            H256(kzg_info.to_blob_commitment())
        })
        .collect::<Vec<_>>();

    // Depending on the length of `pubdata_input`, we will sending the ceiling of
    // `pubdata_input / ZK_SYNC_BYTES_PER_BLOB (126976)` blobs. The rest of the blob commitments will be 32 zero bytes.
    blob_commitments.resize(num_blobs, H256::zero());
    blob_commitments
}
