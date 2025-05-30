//! Tests for the Merkle tree API.

use std::{net::Ipv4Addr, time::Duration};

use assert_matches::assert_matches;
use tempfile::TempDir;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpSocket},
};
use zksync_dal::{ConnectionPool, Core};

use super::*;
use crate::tests::{gen_storage_logs, reset_db_state, run_calculator, setup_calculator};

#[tokio::test]
async fn merkle_tree_api() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, _) = setup_calculator(temp_dir.path(), pool.clone(), true).await;
    let api_addr = (Ipv4Addr::LOCALHOST, 0).into();

    reset_db_state(&pool, 5).await;
    let tree_reader = calculator.tree_reader();
    let calculator_task = tokio::spawn(run_calculator(calculator));

    let (stop_sender, stop_receiver) = watch::channel(false);
    let api_server = tree_reader
        .wait()
        .await
        .unwrap()
        .create_api_server(&api_addr, stop_receiver.clone())
        .await
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
    let TreeApiError::NoVersion {
        missing_version,
        version_count,
    } = err
    else {
        panic!("Unexpected error: {err:?}");
    };
    assert_eq!(version_count, 6);
    assert_eq!(missing_version, 10);

    let raw_nodes_response = api_client
        .inner
        .post(format!("http://{local_addr}/debug/nodes"))
        .json(&serde_json::json!({ "keys": ["0:", "0:0"] }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let raw_nodes_response: serde_json::Value = raw_nodes_response.json().await.unwrap();
    assert_raw_nodes_response(&raw_nodes_response);

    let raw_stale_keys_response = api_client
        .inner
        .post(format!("http://{local_addr}/debug/stale-keys"))
        .json(&serde_json::json!({ "l1_batch_number": 1 }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let raw_stale_keys_response: serde_json::Value = raw_stale_keys_response.json().await.unwrap();
    assert_raw_stale_keys_response(&raw_stale_keys_response);

    let raw_stale_keys_response = api_client
        .inner
        .post(format!("http://{local_addr}/debug/stale-keys/bogus"))
        .json(&serde_json::json!({ "l1_batch_number": 1 }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let raw_stale_keys_response: serde_json::Value = raw_stale_keys_response.json().await.unwrap();
    assert_eq!(
        raw_stale_keys_response,
        serde_json::json!({ "stale_keys": [] })
    );

    // Stop the calculator and the tree API server.
    stop_sender.send_replace(true);
    api_server_task.await.unwrap().unwrap();
}

fn assert_raw_nodes_response(response: &serde_json::Value) {
    let response = response.as_object().expect("not an object");
    let response = response["nodes"].as_object().expect("not an object");
    let root = response["0:"].as_object().expect("not an object");
    assert!(
        root.len() == 2 && root.contains_key("internal") && root.contains_key("raw"),
        "{root:#?}"
    );
    let root = root["internal"].as_object().expect("not an object");
    for key in root.keys() {
        assert_eq!(key.len(), 1, "{key}");
        let key = key.as_bytes()[0];
        assert_matches!(key, b'0'..=b'9' | b'a'..=b'f');
    }

    if let Some(value) = response.get("0:0") {
        let node = value.as_object().expect("not an object");
        assert!(
            node.len() == 2
                && (node.contains_key("internal") || node.contains_key("leaf"))
                && node.contains_key("raw"),
            "{node:#?}"
        );
    }
}

fn assert_raw_stale_keys_response(response: &serde_json::Value) {
    let response = response.as_object().expect("not an object");
    let stale_keys = response["stale_keys"].as_array().expect("not an array");
    assert!(!stale_keys.is_empty()); // At least the root is always obsoleted
    for stale_key in stale_keys {
        let stale_key = stale_key.as_str().expect("not a string");
        stale_key.parse::<NodeKey>().unwrap();
    }
}

#[tokio::test]
async fn api_client_connection_error() {
    // Use an address that will definitely fail on a timeout.
    let socket = TcpSocket::new_v4().unwrap();
    socket.bind((Ipv4Addr::LOCALHOST, 0).into()).unwrap();
    let local_addr = socket.local_addr().unwrap();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    let api_client = TreeApiHttpClient::from_client(client, &format!("http://{local_addr}"));
    let err = api_client.get_info().await.unwrap_err();
    assert_matches!(err, TreeApiError::NotReady(Some(_)));
}

#[tokio::test]
async fn api_client_unparesable_response_error() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            stream
                .write_all(b"HTTP/1.1 200 OK\ncontent-type: application/json\ncontent-length: 13\n\nNot JSON, lol")
                .await
                .ok();
        }
    });

    let api_client = TreeApiHttpClient::new(&format!("http://{local_addr}"));
    let err = api_client.get_info().await.unwrap_err();
    assert_matches!(err, TreeApiError::Internal(_));
}

#[tokio::test]
async fn local_merkle_tree_client() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, _) = setup_calculator(temp_dir.path(), pool.clone(), true).await;

    reset_db_state(&pool, 5).await;
    let tree_reader = calculator.tree_reader();

    let err = tree_reader.get_info().await.unwrap_err();
    assert_matches!(err, TreeApiError::NotReady(None));

    // Wait until the calculator processes initial L1 batches.
    run_calculator(calculator).await;

    let tree_info = tree_reader.get_info().await.unwrap();
    assert!(tree_info.leaf_count > 20);
    assert_eq!(tree_info.next_l1_batch_number, L1BatchNumber(6));

    let err = tree_reader
        .get_proofs(L1BatchNumber(10), vec![])
        .await
        .unwrap_err();
    let TreeApiError::NoVersion {
        missing_version,
        version_count,
    } = err
    else {
        panic!("Unexpected error: {err:?}");
    };
    assert_eq!(version_count, 6);
    assert_eq!(missing_version, 10);
}
