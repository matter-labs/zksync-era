use std::convert::TryInto;

use c_kzg::{Blob, Bytes32, Bytes48, KzgCommitment, KzgProof, KzgSettings, BYTES_PER_BLOB};
use zk_evm::{
    sha2::Sha256,
    sha3::{Digest, Keccak256},
};
use zkevm_circuits::eip_4844::{
    input::{BLOB_CHUNK_SIZE, ELEMENTS_PER_4844_BLOCK},
    zksync_pubdata_into_ethereum_4844_data,
};

const BYTES_PER_BLOB_ZK_SYNC: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;

/// Packed pubdata commitments.
/// Format: opening point (16 bytes) || claimed value (32 bytes) || commitment (48 bytes) || opening proof (48 bytes)) = 144 bytes
const BYTES_PER_PUBDATA_COMMITMENT: usize = 144;

const VERSIONED_HASH_VERSION_KZG: u8 = 0x01;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct KzgInfo {
    pub blob: Blob,
    pub kzg_commitment: Bytes48,
    pub opening_point: Bytes32,
    pub opening_value: Bytes32,
    pub opening_proof: Bytes48,
    pub versioned_hash: Bytes32,
    pub blob_proof: Bytes48,
}

impl KzgInfo {
    /// Size of KzgInfo is equal to size(blob) + size(kzg_commitment) + size(bytes32) + size(bytes32) + size(kzg_proof) + size(bytes32) + size(kzg_proof)
    const SERIALIZED_SIZE: usize = 131_072 + 48 + 32 + 32 + 48 + 32 + 48;

    /// Load in local kzg settings file at trusted_setup.txt
    pub fn kzg_settings() -> KzgSettings {
        let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let path = std::path::Path::new(&zksync_home).join("trusted_setup.txt");
        KzgSettings::load_trusted_setup_file(&path).unwrap()
    }

    /// Returns the bytes necessary for pubdata commitment part of batch commitments when blobs are used.
    /// Return format: opening point (16 bytes) || claimed value (32 bytes) || commitment (48 bytes) || opening proof (48 bytes))
    pub fn to_pubdata_commitment(&self) -> [u8; BYTES_PER_PUBDATA_COMMITMENT] {
        let mut res = [0u8; BYTES_PER_PUBDATA_COMMITMENT];
        // The crypto team/batchCommitment expects the opening point to be 16 bytes
        let mut truncated_opening_point = [0u8; 16];
        truncated_opening_point.copy_from_slice(&self.opening_point.as_slice()[16..]);
        res[0..16].copy_from_slice(&truncated_opening_point);
        res[16..48].copy_from_slice(&self.opening_value.as_slice());
        res[48..96].copy_from_slice(&self.kzg_commitment.as_slice());
        res[96..144].copy_from_slice(&self.opening_proof.as_slice());
        res
    }

    /// Deserializes `Self::SERIALIZED_SIZE` bytes into KzgInfo struct
    pub fn from_slice(data: &[u8]) -> Self {
        assert_eq!(data.len(), Self::SERIALIZED_SIZE);

        let mut ptr = 0;

        let mut blob = [0u8; BYTES_PER_BLOB];
        blob.copy_from_slice(&data[ptr..ptr + BYTES_PER_BLOB]);
        ptr += BYTES_PER_BLOB;

        let mut kzg_commitment = [0u8; 48];
        kzg_commitment.copy_from_slice(&data[ptr..ptr + 48]);
        ptr += 48;

        let mut opening_point = [0u8; 32];
        opening_point.copy_from_slice(&data[ptr..ptr + 32]);
        ptr += 32;

        let mut opening_value = [0u8; 32];
        opening_value.copy_from_slice(&data[ptr..ptr + 32]);
        ptr += 32;

        let mut opening_proof = [0u8; 48];
        opening_proof.copy_from_slice(&data[ptr..ptr + 48]);
        ptr += 48;

        let mut versioned_hash = [0u8; 32];
        versioned_hash.copy_from_slice(&data[ptr..ptr + 32]);
        ptr += 32;

        let mut blob_proof = [0u8; 48];
        blob_proof.copy_from_slice(&data[ptr..ptr + 48]);
        ptr += 48;

        assert_eq!(ptr, Self::SERIALIZED_SIZE);

        Self {
            blob: Blob::new(blob),
            kzg_commitment: Bytes48::from_bytes(&kzg_commitment).unwrap(),
            opening_point: Bytes32::new(opening_point),
            opening_value: Bytes32::new(opening_value),
            opening_proof: Bytes48::from_bytes(&opening_proof).unwrap(),
            versioned_hash: Bytes32::new(versioned_hash),
            blob_proof: Bytes48::from_bytes(&blob_proof).unwrap(),
        }
    }

    /// Converts `KzgInfo` struct into a byte array
    pub fn to_bytes(&self) -> [u8; Self::SERIALIZED_SIZE] {
        let mut res = [0u8; Self::SERIALIZED_SIZE];

        let mut ptr = 0;

        res[ptr..ptr + BYTES_PER_BLOB].copy_from_slice(self.blob.as_slice());
        ptr += BYTES_PER_BLOB;

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
    ///     1. zksync blob <- pad_right(pubdata)
    ///     2. linear hash <- hash(zksync blob)
    ///     3. 4844 blob <- zksync_pubdata_into_ethereum_4844_data(zksync blob)
    ///     4. 4844 kzg commitment <- blob_to_kzg_commitment(4844 blob)
    ///     5. versioned hash <- hash(4844 kzg commitment)
    ///     6. opening point <- keccak(linear hash || versioned hash)[16..]
    ///     7. opening value, opening proof <- compute_kzg_proof(4844)
    ///     8. blob proof <- compute_blob_kzg_proof(blob, 4844 kzg commitment)
    pub fn new(pubdata: Vec<u8>) -> Self {
        let kzg_settings = Self::kzg_settings();

        assert!(pubdata.len() <= BYTES_PER_BLOB_ZK_SYNC);

        let mut zksync_blob = [0u8; BYTES_PER_BLOB_ZK_SYNC];
        zksync_blob[0..pubdata.len()].copy_from_slice(&pubdata);

        let mut hasher = Keccak256::new();
        hasher.update(zksync_blob);
        let linear_hash = &hasher.finalize_reset();

        let bytes_4844 = zksync_pubdata_into_ethereum_4844_data(&zksync_blob);
        let blob = Blob::new(bytes_4844.try_into().unwrap());

        let kzg_commitment = KzgCommitment::blob_to_kzg_commitment(&blob, &kzg_settings).unwrap();

        let mut sha256_hasher = Sha256::new();
        sha256_hasher.update(&kzg_commitment.to_bytes().into_inner());
        let mut versioned_hash_bytes = sha256_hasher.finalize();
        versioned_hash_bytes[0] = VERSIONED_HASH_VERSION_KZG;
        let versioned_hash = Bytes32::from_bytes(&versioned_hash_bytes).unwrap();

        hasher.update(linear_hash.as_slice());
        hasher.update(versioned_hash_bytes);

        let opening_point_bytes = &hasher.finalize_reset();
        let mut opening_point = [0u8; 32];
        opening_point[16..].copy_from_slice(&opening_point_bytes[16..]);
        let opening_point = Bytes32::from_bytes(&opening_point).unwrap();

        let (opening_proof, opening_value) =
            KzgProof::compute_kzg_proof(&blob, &opening_point, &kzg_settings).unwrap();

        let blob_proof =
            KzgProof::compute_blob_kzg_proof(&blob, &kzg_commitment.to_bytes(), &kzg_settings)
                .unwrap();

        Self {
            blob,
            kzg_commitment: kzg_commitment.to_bytes(),
            opening_point,
            opening_value,
            opening_proof: opening_proof.to_bytes(),
            versioned_hash,
            blob_proof: blob_proof.to_bytes(),
        }
    }

    pub fn kzg_commitment(&self) -> KzgCommitment {
        KzgCommitment::from_bytes(self.kzg_commitment.as_slice()).unwrap()
    }

    pub fn opening_proof(&self) -> KzgProof {
        KzgProof::from_bytes(self.opening_proof.as_slice()).unwrap()
    }

    pub fn blob_proof(&self) -> KzgProof {
        KzgProof::from_bytes(self.blob_proof.as_slice()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_with::serde_as;

    use super::{KzgInfo, KzgProof};
    use crate::{H256, U256};

    #[serde_as]
    #[derive(Debug, Serialize, Deserialize)]
    struct ExepectedOutputs {
        versioned_hash: H256,
        #[serde_as(as = "serde_with::hex::Hex")]
        kzg_commitment: Vec<u8>,
        opening_point: U256,
        opening_value: U256,
        #[serde_as(as = "serde_with::hex::Hex")]
        opening_proof: Vec<u8>,
        #[serde_as(as = "serde_with::hex::Hex")]
        blob_proof: Vec<u8>,
        #[serde_as(as = "serde_with::hex::Hex")]
        pubdata_commitment: Vec<u8>,
    }

    impl From<KzgInfo> for ExepectedOutputs {
        fn from(value: KzgInfo) -> Self {
            let kzg_commitment = value.kzg_commitment.as_slice().to_vec();
            let opening_point = U256::from(value.opening_point.as_slice());
            let opening_value = U256::from(value.opening_value.as_slice());
            let versioned_hash = H256::from_slice(value.versioned_hash.as_slice());
            let opening_proof = value.opening_proof.as_slice().to_vec();
            let blob_proof = value.blob_proof.as_slice().to_vec();

            Self {
                kzg_commitment,
                opening_point,
                opening_value,
                versioned_hash,
                opening_proof,
                blob_proof,
                pubdata_commitment: vec![],
            }
        }
    }

    impl PartialEq for ExepectedOutputs {
        fn eq(&self, other: &Self) -> bool {
            self.versioned_hash == other.versioned_hash
                && self.kzg_commitment == other.kzg_commitment
                && self.opening_point == other.opening_point
                && self.opening_value == other.opening_value
                && self.opening_proof == other.opening_proof
                && self.blob_proof == other.blob_proof
        }
    }

    #[serde_as]
    #[derive(Debug, Serialize, Deserialize)]
    struct KzgTest {
        #[serde_as(as = "serde_with::hex::Hex")]
        pubdata: Vec<u8>,
        expected_outputs: ExepectedOutputs,
    }

    #[test]
    fn kzg_test() {
        let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let path = std::path::Path::new(&zksync_home).join("etc/kzg_tests/kzg_test_0.json");
        let contents = std::fs::read_to_string(path).unwrap();
        let kzg_test: KzgTest = serde_json::from_str(&contents).unwrap();

        let kzg_info = KzgInfo::new(kzg_test.pubdata);

        assert_eq!(
            kzg_test.expected_outputs,
            ExepectedOutputs::from(kzg_info.clone())
        );

        let encoded_info = kzg_info.to_bytes();
        let kzg_info_2 = KzgInfo::from_slice(&encoded_info);

        assert_eq!(kzg_info, kzg_info_2);

        let pubdata_commitment = kzg_info.to_pubdata_commitment();

        assert_eq!(
            kzg_test.expected_outputs.pubdata_commitment,
            pubdata_commitment.to_vec()
        );

        let point_proof_verify = KzgProof::verify_kzg_proof(
            &kzg_info.kzg_commitment,
            &kzg_info.opening_point,
            &kzg_info.opening_value,
            &kzg_info.opening_proof,
            &KzgInfo::kzg_settings(),
        );

        assert!(point_proof_verify.is_ok());
        assert!(point_proof_verify.unwrap());

        let blob_proof_verify = KzgProof::verify_blob_kzg_proof(
            &kzg_info.blob,
            &kzg_info.kzg_commitment,
            &kzg_info.blob_proof,
            &KzgInfo::kzg_settings(),
        );

        assert!(blob_proof_verify.is_ok());
        assert!(blob_proof_verify.unwrap());
    }
}
