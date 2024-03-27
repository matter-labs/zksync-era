//! Tests for the Merkle tree API.

use std::net::Ipv4Addr;

use assert_matches::assert_matches;
use tempfile::TempDir;
use zksync_dal::{ConnectionPool, Core};

use super::*;
use crate::metadata_calculator::tests::{
    gen_storage_logs, reset_db_state, run_calculator, setup_calculator,
};

#[tokio::test]
async fn merkle_tree_api() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, _) = setup_calculator(temp_dir.path(), &pool).await;
    let api_addr = (Ipv4Addr::LOCALHOST, 0).into();

    reset_db_state(&pool, 5).await;
    let tree_reader = calculator.tree_reader();
    let calculator_task = tokio::spawn(run_calculator(calculator, pool));

    let (stop_sender, stop_receiver) = watch::channel(false);
    let api_server = tree_reader
        .wait()
        .await
        .create_api_server(&api_addr, stop_receiver.clone())
        .unwrap();
    let local_addr = *api_server.local_addr();
    let api_server_task = tokio::spawn(api_server.run());
    let api_client = TreeApiHttpClient::new(&format!("http://{local_addr}"));

    // Wait until the calculator processes initial L1 batches.
    calculator_task.await.unwrap();

    // Query the API.
    let tree_info = api_client.get_info().await.unwrap();
    assert!(tree_info.leaf_count > 20);
    assert_eq!(tree_info.next_l1_batch_number, L1BatchNumber(6));

    let mut hashed_keys: Vec<_> = gen_storage_logs(20..30, 1)[0]
        .iter()
        .map(|log| log.key.hashed_key_u256())
        .collect();
    // Extend with some non-existing keys.
    hashed_keys.extend((0_u8..10).map(|byte| U256::from_big_endian(&[byte; 32])));

    let proofs = api_client
        .get_proofs(L1BatchNumber(5), hashed_keys)
        .await
        .unwrap();
    assert_eq!(proofs.len(), 20);
    for (i, proof) in proofs.into_iter().enumerate() {
        let should_be_present = i < 10;
        assert_eq!(proof.index == 0, !should_be_present);
        assert!(!proof.merkle_path.is_empty());
    }

    let err = api_client
        .get_proofs(L1BatchNumber(10), vec![])
        .await
        .unwrap_err();
    let TreeApiError::NoVersion(err) = err else {
        panic!("Unexpected error: {err:?}");
    };
    assert_eq!(err.version_count, 6);
    assert_eq!(err.missing_version, 10);

    // Stop the calculator and the tree API server.
    stop_sender.send_replace(true);
    api_server_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn local_merkle_tree_client() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, _) = setup_calculator(temp_dir.path(), &pool).await;

    reset_db_state(&pool, 5).await;
    let tree_reader = calculator.tree_reader();

    let err = tree_reader.get_info().await.unwrap_err();
    assert_matches!(err, TreeApiError::NotReady);

    // Wait until the calculator processes initial L1 batches.
    run_calculator(calculator, pool).await;

    let tree_info = tree_reader.get_info().await.unwrap();
    assert!(tree_info.leaf_count > 20);
    assert_eq!(tree_info.next_l1_batch_number, L1BatchNumber(6));

    let err = tree_reader
        .get_proofs(L1BatchNumber(10), vec![])
        .await
        .unwrap_err();
    let TreeApiError::NoVersion(err) = err else {
        panic!("Unexpected error: {err:?}");
    };
    assert_eq!(err.version_count, 6);
    assert_eq!(err.missing_version, 10);
}
