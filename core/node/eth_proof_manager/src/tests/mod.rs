use std::time::Duration;

use zksync_config::{
    configs::{
        eth_proof_manager::EthProofManagerConfig, object_store::ObjectStoreMode,
        proof_data_handler::ProvingMode,
    },
    ObjectStoreConfig,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
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

#[tokio::test]
async fn test_flow() {
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
}
