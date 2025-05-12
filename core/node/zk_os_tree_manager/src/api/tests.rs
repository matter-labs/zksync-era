//! Tests for the tree API server / client.

use std::net::Ipv4Addr;

use assert_matches::assert_matches;
use tempfile::TempDir;
use zk_os_merkle_tree::{BatchOutput, Blake2Hasher, DefaultTreeParams, TreeParams};
use zksync_dal::{ConnectionPool, Core};

use super::*;
use crate::tests::setup_tree_manager;

#[tokio::test]
async fn in_process_client_basics() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed to get temporary directory for RocksDB");

    let tree_manager = setup_tree_manager(temp_dir.path(), pool).await;
    let client = tree_manager.tree_reader();
    let mut batches_subscriber = tree_manager.subscribe_to_l1_batches();
    // The client shouldn't be ready until the manager is started.
    let err = client.get_info().await.unwrap_err();
    assert_matches!(err, TreeApiError::NotReady(_));

    let (_stop_sender, stop_receiver) = watch::channel(false);
    tokio::spawn(tree_manager.run(stop_receiver));

    // Wait until the genesis batch is processed.
    batches_subscriber
        .wait_for(|&batch| batch == L1BatchNumber(1))
        .await
        .unwrap();

    let info = client.get_info().await.unwrap();
    assert_eq!(info.min_version, Some(0));
    assert_eq!(info.next_version, 1);
    assert!(info.leaf_count > 10, "{info:?}");

    let keys = vec![H256::zero(), H256::repeat_byte(1), H256::repeat_byte(0xaa)];
    let proof = client
        .get_proof(L1BatchNumber(0), keys.clone())
        .await
        .unwrap();
    let tree_output = BatchOutput {
        root_hash: info.root_hash,
        leaf_count: info.leaf_count,
    };
    proof
        .verify_reads(
            &Blake2Hasher,
            <DefaultTreeParams>::TREE_DEPTH,
            tree_output,
            &keys,
        )
        .unwrap();
}

#[tokio::test]
async fn api_client_basics() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed to get temporary directory for RocksDB");

    let tree_manager = setup_tree_manager(temp_dir.path(), pool).await;
    let tree_reader = tree_manager.tree_reader();
    let mut batches_subscriber = tree_manager.subscribe_to_l1_batches();
    let (stop_sender, stop_receiver) = watch::channel(false);
    tokio::spawn(tree_manager.run(stop_receiver.clone()));

    batches_subscriber
        .wait_for(|&batch| batch == L1BatchNumber(1))
        .await
        .unwrap();

    let tree_reader = tree_reader.wait().await.unwrap();
    let bind_address = (Ipv4Addr::LOCALHOST, 0).into();
    let server = tree_reader
        .create_api_server(&bind_address, stop_receiver)
        .await
        .unwrap();
    let api_base_url = format!("http://{}", server.local_addr());
    let server_task = tokio::spawn(server.run());

    let api_client = TreeApiHttpClient::new(&api_base_url);
    let info = api_client.get_info().await.unwrap();
    assert_eq!(info.min_version, Some(0));
    assert_eq!(info.next_version, 1);
    assert!(info.leaf_count > 10, "{info:?}");

    let keys = vec![H256::zero(), H256::repeat_byte(1), H256::repeat_byte(0xaa)];
    let proof = api_client
        .get_proof(L1BatchNumber(0), keys.clone())
        .await
        .unwrap();
    let tree_output = BatchOutput {
        root_hash: info.root_hash,
        leaf_count: info.leaf_count,
    };
    proof
        .verify_reads(
            &Blake2Hasher,
            <DefaultTreeParams>::TREE_DEPTH,
            tree_output,
            &keys,
        )
        .unwrap();

    // Check raw nodes API
    let request = serde_json::json!({
        "keys": [
            "0:0:0", // root
            "0:22:1", // max guard leaf
            "0:21:0", // parent node for guards
        ],
    });
    let raw_nodes: TreeNodesResponse = api_client.post("/debug/nodes", &request).await.unwrap();
    let root_node = &raw_nodes.nodes[&"0:0:0".parse::<unstable::NodeKey>().unwrap().into()];
    let root_node = root_node.internal.as_ref().unwrap();
    assert_eq!(root_node.0.len(), 1);
    let leaf_node = &raw_nodes.nodes[&"0:22:1".parse::<unstable::NodeKey>().unwrap().into()];
    let leaf_node = leaf_node.leaf.as_ref().unwrap();
    assert_eq!(leaf_node.key, H256::repeat_byte(0xff));
    assert_eq!(leaf_node.value, H256::zero());
    assert_eq!(leaf_node.next_index, 1);
    let internal_node = &raw_nodes.nodes[&"0:21:0".parse::<unstable::NodeKey>().unwrap().into()];
    let internal_node = internal_node.internal.as_ref().unwrap();
    assert!(internal_node.0.len() > 1, "{internal_node:?}");

    // Check that the server reacts to a stop request.
    stop_sender.send_replace(true);
    server_task.await.unwrap().unwrap();
}
