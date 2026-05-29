pub use kzg::{pubdata_to_blob_commitments, KzgInfo, ZK_SYNC_BYTES_PER_BLOB};
use zksync_types::{web3::keccak256, H256};

/// Compute the EIP-4844 versioned hash for each blob in `pubdata_input`.
///
/// Layout mirrors [`pubdata_to_blob_commitments`]: pubdata is split into chunks
/// of `ZK_SYNC_BYTES_PER_BLOB`, each chunk run through [`KzgInfo::new`], and the
/// resulting `versioned_hash` (`0x01 || sha256(kzg_commitment)[1..32]`)
/// collected as `H256`. Slots beyond the pubdata are zero-filled to reach
/// `num_blobs`.
pub fn pubdata_to_blob_versioned_hashes(num_blobs: usize, pubdata_input: &[u8]) -> Vec<H256> {
    assert!(
        pubdata_input.len() <= num_blobs * ZK_SYNC_BYTES_PER_BLOB,
        "pubdata length exceeds size of blobs"
    );
    let mut hashes = pubdata_input
        .chunks(ZK_SYNC_BYTES_PER_BLOB)
        .map(|blob| H256(KzgInfo::new(blob).versioned_hash))
        .collect::<Vec<_>>();
    hashes.resize(num_blobs, H256::zero());
    hashes
}

/// Compute the keccak256 linear hash for each blob in `pubdata_input`.
///
/// Pubdata is padded to a multiple of `ZK_SYNC_BYTES_PER_BLOB`; the resulting
/// vector has `blobs_required` entries, with empty slots set to `H256::zero()`.
pub fn pubdata_to_blob_linear_hashes(
    blobs_required: usize,
    mut pubdata_input: Vec<u8>,
) -> Vec<H256> {
    if pubdata_input.len() % ZK_SYNC_BYTES_PER_BLOB != 0 {
        pubdata_input.resize(
            pubdata_input.len()
                + (ZK_SYNC_BYTES_PER_BLOB - pubdata_input.len() % ZK_SYNC_BYTES_PER_BLOB),
            0,
        );
    }

    let mut result = vec![H256::zero(); blobs_required];
    pubdata_input
        .chunks(ZK_SYNC_BYTES_PER_BLOB)
        .enumerate()
        .for_each(|(i, chunk)| {
            result[i] = H256(keccak256(chunk));
        });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Versioned hashes for a populated slot must carry the EIP-4844 prefix
    /// (`0x01`) and empty slots must be zero. Asserting an exact value would
    /// require pinning the trusted setup; this is enough to catch obvious
    /// regressions in the helper without coupling to the setup file.
    #[test]
    fn versioned_hashes_have_eip4844_prefix() {
        let pubdata = vec![0u8; ZK_SYNC_BYTES_PER_BLOB];
        let versioned = pubdata_to_blob_versioned_hashes(2, &pubdata);
        assert_eq!(versioned.len(), 2);
        assert_eq!(
            versioned[0].as_bytes()[0],
            0x01,
            "slot 0 must have EIP-4844 prefix"
        );
        assert_eq!(versioned[1], H256::zero(), "empty slot must be zero");
    }

    /// The helper's output must equal what `KzgInfo::new(blob).versioned_hash`
    /// produces directly — proving the helper is just batched delegation.
    #[test]
    fn versioned_hashes_match_kzg_info() {
        let mut pubdata = vec![0u8; ZK_SYNC_BYTES_PER_BLOB];
        for (i, byte) in pubdata.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }

        let versioned = pubdata_to_blob_versioned_hashes(1, &pubdata);
        let direct = H256(KzgInfo::new(&pubdata).versioned_hash);
        assert_eq!(versioned[0], direct);
    }

    #[test]
    fn versioned_hashes_resize_to_num_blobs() {
        let pubdata = vec![];
        let versioned = pubdata_to_blob_versioned_hashes(16, &pubdata);
        assert_eq!(versioned.len(), 16);
        assert!(versioned.iter().all(|h| *h == H256::zero()));
    }

    /// Linear hashes for a small pubdata must produce keccak256 of the
    /// zero-padded blob. Compares against keccak256 directly to lock in the
    /// padding contract.
    #[test]
    fn linear_hashes_keccak_zero_padded_blob() {
        let pubdata = vec![0xABu8; 100];
        let linear = pubdata_to_blob_linear_hashes(2, pubdata.clone());
        assert_eq!(linear.len(), 2);

        let mut padded = pubdata.clone();
        padded.resize(ZK_SYNC_BYTES_PER_BLOB, 0);
        assert_eq!(linear[0], H256(keccak256(&padded)));
        assert_eq!(linear[1], H256::zero(), "empty slot must be zero");
    }

    #[test]
    fn linear_hashes_resize_to_blobs_required() {
        let pubdata = vec![];
        let linear = pubdata_to_blob_linear_hashes(16, pubdata);
        assert_eq!(linear.len(), 16);
        assert!(linear.iter().all(|h| *h == H256::zero()));
    }
}
