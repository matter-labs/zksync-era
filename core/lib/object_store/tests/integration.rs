//! Integration tests for object store.

use tokio::fs;

use zksync_object_store::{Bucket, ObjectStoreFactory};
use zksync_types::{
    proofs::{PrepareBasicCircuitsJob, StorageLogMetadata},
    L1BatchNumber,
};

/// Tests compatibility of the `PrepareBasicCircuitsJob` serialization to the previously used
/// one.
#[tokio::test]
async fn prepare_basic_circuits_job_serialization() {
    // The working dir for integration tests is set to the crate dir, so specifying relative paths
    // should be OK.
    let snapshot = fs::read("./tests/snapshots/prepare-basic-circuits-job-full.bin")
        .await
        .unwrap();
    let store = ObjectStoreFactory::mock().create_store().await;
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
