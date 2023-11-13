use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_types::aggregated_operations::L1BatchProofForL1;
use zksync_types::L1BatchNumber;

pub async fn load_wrapped_fri_proofs_for_range(
    from: L1BatchNumber,
    to: L1BatchNumber,
    blob_store: &dyn ObjectStore,
) -> Vec<L1BatchProofForL1> {
    let mut proofs = Vec::new();
    for l1_batch_number in from.0..=to.0 {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        match blob_store.get(l1_batch_number).await {
            Ok(proof) => proofs.push(proof),
            Err(ObjectStoreError::KeyNotFound(_)) => (), // do nothing, proof is not ready yet
            Err(err) => panic!(
                "Failed to load proof for batch {}: {}",
                l1_batch_number.0, err
            ),
        }
    }
    proofs
}
