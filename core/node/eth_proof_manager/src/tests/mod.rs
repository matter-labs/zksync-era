use std::time::Duration;

use zksync_config::{
    configs::{
        eth_proof_manager::EthProofManagerConfig, object_store::ObjectStoreMode,
        proof_data_handler::ProvingMode,
    },
    ObjectStoreConfig,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{eth_proof_manager_dal::ProvingNetwork, Connection, ConnectionPool, Core, CoreDal};
use zksync_object_store::MockObjectStore;
use zksync_proof_data_handler::{Locking, Processor};
use zksync_types::{
    block::{L1BatchHeader, L1BatchTreeData},
    commitment::L1BatchCommitmentArtifacts,
    L1BatchNumber, L2ChainId, ProtocolVersion, ProtocolVersionId, H256,
};

fn test_config() -> EthProofManagerConfig {
    EthProofManagerConfig {
        sl_chain_id: 270,
        http_rpc_url: "http://127.0.0.1:3050".to_string(),
        object_store: ObjectStoreConfig {
            mode: ObjectStoreMode::FileBacked {
                file_backed_base_path: "/tmp/zksync/object_store".to_string().into(),
            },
            max_retries: 10,
            local_mirror_path: None,
        },
        event_poll_interval: Duration::from_secs(1),
        request_sending_interval: Duration::from_secs(1),
        event_expiration_blocks: 100,
        default_priority_fee_per_gas: 1000000000,
        max_reward: 1000000000,
        acknowledgment_timeout: Duration::from_secs(1),
        proof_generation_timeout: Duration::from_secs(1),
        picking_timeout: Duration::from_secs(1),
        max_acceptable_priority_fee_in_gwei: 1000000000,
        max_tx_sending_attempts: 10,
        tx_sending_sleep: Duration::from_secs(1),
        tx_receipt_checking_max_attempts: 10,
        tx_receipt_checking_sleep: Duration::from_secs(1),
        max_tx_gas: 1000000000,
        path_to_fflonk_verification_key:
            "./core/node/eth_proof_manager/src/tests/fflonk_verification_key.json"
                .to_string()
                .into(),
    }
}

fn create_l1_batch_header(number: u32) -> L1BatchHeader {
    L1BatchHeader::new(
        L1BatchNumber(number),
        100,
        BaseSystemContractsHashes {
            bootloader: H256::repeat_byte(1),
            default_aa: H256::repeat_byte(42),
            evm_emulator: Some(H256::repeat_byte(43)),
        },
        ProtocolVersionId::latest(),
    )
}

async fn prepare_database(conn: &mut Connection<'_, Core>) {
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    conn.blocks_dal()
        .insert_mock_l1_batch(&create_l1_batch_header(1))
        .await
        .unwrap();

    let unpicked_l1_batch = conn
        .proof_generation_dal()
        .get_oldest_unpicked_batch()
        .await
        .unwrap();
    assert_eq!(unpicked_l1_batch, None);

    conn.proof_generation_dal()
        .insert_proof_generation_details(L1BatchNumber(1))
        .await
        .unwrap();

    let unpicked_l1_batch = conn
        .proof_generation_dal()
        .get_oldest_unpicked_batch()
        .await
        .unwrap();
    assert_eq!(unpicked_l1_batch, Some(L1BatchNumber(1)));

    // Calling the method multiple times should work fine.
    conn.proof_generation_dal()
        .insert_proof_generation_details(L1BatchNumber(1))
        .await
        .unwrap();
    conn.proof_generation_dal()
        .save_vm_runner_artifacts_metadata(L1BatchNumber(1), "vm_run")
        .await
        .unwrap();
    conn.proof_generation_dal()
        .save_merkle_paths_artifacts_metadata(L1BatchNumber(1), "data")
        .await
        .unwrap();
    conn.blocks_dal()
        .save_l1_batch_tree_data(
            L1BatchNumber(1),
            &L1BatchTreeData {
                hash: H256::zero(),
                rollup_last_leaf_index: 123,
            },
        )
        .await
        .unwrap();
    conn.blocks_dal()
        .save_l1_batch_commitment_artifacts(
            L1BatchNumber(1),
            &L1BatchCommitmentArtifacts::default(),
        )
        .await
        .unwrap();
}

// test basic flow of proof manager with fallbacking due to acknowledgment timeout
#[tokio::test]
async fn test_flow_acknowledgment_timeout() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let config = test_config();
    let blob_store = MockObjectStore::arc();

    let mut connection = connection_pool.connection().await.unwrap();

    prepare_database(&mut connection).await;

    let processor = Processor::<Locking>::new(
        blob_store,
        connection_pool,
        config.proof_generation_timeout,
        L2ChainId::new(270).unwrap(),
        ProvingMode::ProvingNetwork,
    );

    // At this point, batch should be available for proving networks, but not for prover cluster

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();

    assert_eq!(batch, None);

    let batch = processor.lock_batch_for_proving_network().await.unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));

    processor.unlock_batch(L1BatchNumber(1)).await.unwrap();

    connection
        .eth_proof_manager_dal()
        .insert_batch(L1BatchNumber(1), "url")
        .await
        .unwrap();

    connection.eth_proof_manager_dal().mark_batch_as_sent(L1BatchNumber(1), H256::zero()).await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    // After timeout of acknowledgment, batch should be fallbacked, so it will be available for prover cluster, but not for proving network

    connection
        .eth_proof_manager_dal()
        .fallback_batches(
            config.acknowledgment_timeout,
            config.proof_generation_timeout,
            config.picking_timeout,
        )
        .await
        .unwrap();

    let batch = processor.lock_batch_for_proving_network().await.unwrap();
    assert_eq!(batch, None);

    let batch = processor.lock_batch_for_proving(config.proof_generation_timeout).await.unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));
}

// test basic flow of proof manager with fallbacking due to proving timeout
#[tokio::test]
async fn test_flow_proving_timeout() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut config = test_config();
    config.acknowledgment_timeout = Duration::from_secs(20);
    let blob_store = MockObjectStore::arc();

    let mut connection = connection_pool.connection().await.unwrap();

    prepare_database(&mut connection).await;

    let processor = Processor::<Locking>::new(
        blob_store,
        connection_pool,
        config.proof_generation_timeout,
        L2ChainId::new(270).unwrap(),
        ProvingMode::ProvingNetwork,
    );

    // At this point, batch should be available for proving networks, but not for prover cluster

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();
    assert_eq!(batch, None);

    let batch = processor.lock_batch_for_proving_network().await.unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));

    processor.unlock_batch(L1BatchNumber(1)).await.unwrap();

    connection
        .eth_proof_manager_dal()
        .insert_batch(L1BatchNumber(1), "url")
        .await
        .unwrap();

    connection
        .eth_proof_manager_dal()
        .acknowledge_batch(L1BatchNumber(1), ProvingNetwork::Fermah)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    // After timeout of proving, batch should be fallbacked, so it will be available for prover cluster, but not for proving network

    connection
        .eth_proof_manager_dal()
        .fallback_batches(
            config.acknowledgment_timeout,
            config.proof_generation_timeout,
            config.picking_timeout,
        )
        .await
        .unwrap();

    let batch = processor.lock_batch_for_proving_network().await.unwrap();
    assert_eq!(batch, None);

    let batch = processor.lock_batch_for_proving(config.proof_generation_timeout).await.unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));
}

// test basic flow of proof manager with fallbacking due to picking timeout
#[tokio::test]
async fn test_flow_picking_timeout() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let config = test_config();
    let blob_store = MockObjectStore::arc();

    let mut connection = connection_pool.connection().await.unwrap();

    prepare_database(&mut connection).await;

    let processor = Processor::<Locking>::new(
        blob_store,
        connection_pool,
        config.proof_generation_timeout,
        L2ChainId::new(270).unwrap(),
        ProvingMode::ProvingNetwork,
    );

    // At this point, batch should be available for proving networks, but not for prover cluster

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();

    assert_eq!(batch, None);

    let batch = processor.lock_batch_for_proving_network().await.unwrap();
    assert_eq!(batch, Some(L1BatchNumber(1)));

    processor.unlock_batch(L1BatchNumber(1)).await.unwrap();

    // After timeout of picking, batch should be fallbacked, so it will be available for prover cluster, but not for proving network

    tokio::time::sleep(Duration::from_secs(2)).await;

    connection
        .eth_proof_manager_dal()
        .fallback_batches(
            config.acknowledgment_timeout,
            config.proof_generation_timeout,
            config.picking_timeout,
        )
        .await
        .unwrap();

    let batch = processor.lock_batch_for_proving_network().await.unwrap();
    assert_eq!(batch, None);

    let batch = processor.lock_batch_for_proving(config.proof_generation_timeout).await.unwrap();

    assert_eq!(batch, Some(L1BatchNumber(1)));
}

#[tokio::test]
async fn test_picking_batch_with_prover_cluster_mode() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let config = test_config();
    let blob_store = MockObjectStore::arc();

    let mut connection = connection_pool.connection().await.unwrap();

    prepare_database(&mut connection).await;

    let processor = Processor::<Locking>::new(
        blob_store,
        connection_pool,
        config.proof_generation_timeout,
        L2ChainId::new(270).unwrap(),
        ProvingMode::ProverCluster,
    );

    let batch = processor
        .lock_batch_for_proving(config.proof_generation_timeout)
        .await
        .unwrap();
    assert_eq!(batch, Some(L1BatchNumber(1)));
}

#[tokio::test]
async fn test_picking_batch_with_proving_network_mode() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let config = test_config();
    let blob_store = MockObjectStore::arc();

    let mut connection = connection_pool.connection().await.unwrap();

    prepare_database(&mut connection).await;

    let processor = Processor::<Locking>::new(
        blob_store,
        connection_pool,
        config.proof_generation_timeout,
        L2ChainId::new(270).unwrap(),
        ProvingMode::ProvingNetwork,
    );

    tokio::time::sleep(Duration::from_secs(2)).await;

    let batch = processor
        .lock_batch_for_proving(Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(batch, Some(L1BatchNumber(1)));
}
