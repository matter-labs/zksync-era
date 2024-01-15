use std::path::Path;

use c_kzg::{Blob, Bytes32, KzgCommitment, KzgProof, KzgSettings, BYTES_PER_BLOB};
use zkevm_circuits::eip_4844::input::{BLOB_CHUNK_SIZE, ELEMENTS_PER_4844_BLOCK};

const BYTES_PER_BLOB_ZK_SYNC: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;

/// Packed pubdata commitments.
/// Format: opening point (16 bytes) || claimed value (32 bytes) || commitment (48 bytes) || opening proof (48 bytes)) = 144 bytes
const BYTES_PER_PUBDATA_COMMITMENT: usize = 144;

// const KZG_SETTINGS: KzgSettings = KzgSettings::load_trusted_setup_file(&Path::new("./trusted_setup.txt")).unwrap();

pub struct KzgInfo {
    pub blob: Blob,
    pub kzg_commitment: KzgCommitment,
    pub opening_point: Bytes32,
    pub opening_value: Bytes32,
    pub opening_proof: KzgProof,
    pub versioned_hash: Bytes32,
    pub blob_proof: KzgProof,
}

impl KzgInfo {
    /// Size of KzgInfo is equal to size(blob) + size(kzg_commitment) + size(bytes32) + size(bytes32) + size(kzg_proof) + size(bytes32) + size(kzg_proof)
    const SERIALIZED_SIZE: usize = 131_072 + 48 + 32 + 32 + 48 + 32 + 48;

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
            kzg_commitment: KzgCommitment::from_bytes(&kzg_commitment).unwrap(),
            opening_point: Bytes32::new(opening_point),
            opening_value: Bytes32::new(opening_value),
            opening_proof: KzgProof::from_bytes(&opening_proof).unwrap(),
            versioned_hash: Bytes32::new(versioned_hash),
            blob_proof: KzgProof::from_bytes(&blob_proof).unwrap(),
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
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_with::serde_as;

    #[serde_as]
    #[derive(Debug, Serialize, Deserialize)]
    struct KzgTest {
        #[serde_as(as = "serde_with::hex::Hex")]
        blob: Vec<u8>,
        #[serde_as(as = "serde_with::hex::Hex")]
        versioned_hash: Vec<u8>,
        #[serde_as(as = "serde_with::hex::Hex")]
        commitment: Vec<u8>,
    }

    #[test]
    fn kzg_test() {
        let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let path = std::path::Path::new(&zksync_home).join("etc/kzg_tests/kzg_test_0.json");
        let contents = std::fs::read_to_string(path).unwrap();
        let kzg_test: KzgTest = serde_json::from_str(&contents).unwrap();

        dbg!(kzg_test);
    }
}
