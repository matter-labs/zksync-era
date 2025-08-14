use std::{sync::Arc, time::Duration};

use zksync_config::{
    configs::{
        eth_proof_manager::EthProofManagerConfig, object_store::ObjectStoreMode,
        proof_data_handler::ProvingMode,
    },
    ObjectStoreConfig,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::{MockObjectStore, ObjectStore};
use zksync_proof_data_handler::{Locking, Processor};
use zksync_types::{
    block::{L1BatchHeader, L1BatchTreeData},
    commitment::L1BatchCommitmentArtifacts,
    L1BatchNumber, L2ChainId, ProtocolVersion, ProtocolVersionId, H256,
};

mod fallbacking;

pub(super) struct TestContext {
    pub connection_pool: ConnectionPool<Core>,
    pub config: EthProofManagerConfig,
    pub blob_store: Arc<dyn ObjectStore>,
}

impl TestContext {
    pub async fn new() -> Self {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let config = Self::test_config();
        let blob_store = MockObjectStore::arc();

        Self {
            connection_pool,
            config,
            blob_store,
        }
    }

    pub async fn init(self) -> Self {
        self.prepare_database()
            .await
            .expect("Failed to prepare database");

        self
    }

    pub async fn processor(&self, proving_mode: ProvingMode) -> Processor<Locking> {
        Processor::<Locking>::new(
            self.blob_store.clone(),
            self.connection_pool.clone(),
            self.config.proof_generation_timeout,
            L2ChainId::new(270).unwrap(),
            proving_mode,
        )
    }

    fn test_config() -> EthProofManagerConfig {
        EthProofManagerConfig {
            l2_chain_id: 270,
            http_rpc_url: "http://127.0.0.1:3050".to_string(),
            public_object_store_url: "".to_string(),
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
                "./core/node/eth_proof_manager/src/tests/fflonk_verification_key.json".to_string(),
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

    async fn prepare_database(&self) -> anyhow::Result<()> {
        let mut conn = self.connection_pool.connection().await?;

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await?;

        conn.blocks_dal()
            .insert_mock_l1_batch(&Self::create_l1_batch_header(1))
            .await?;

        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, None);

        conn.proof_generation_dal()
            .insert_proof_generation_details(L1BatchNumber(1))
            .await?;

        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await?;
        assert_eq!(unpicked_l1_batch, Some(L1BatchNumber(1)));

        // Calling the method multiple times should work fine.
        conn.proof_generation_dal()
            .insert_proof_generation_details(L1BatchNumber(1))
            .await?;
        conn.proof_generation_dal()
            .save_vm_runner_artifacts_metadata(L1BatchNumber(1), "vm_run")
            .await?;
        conn.proof_generation_dal()
            .save_merkle_paths_artifacts_metadata(L1BatchNumber(1), "data")
            .await?;
        conn.blocks_dal()
            .save_l1_batch_tree_data(
                L1BatchNumber(1),
                &L1BatchTreeData {
                    hash: H256::zero(),
                    rollup_last_leaf_index: 123,
                },
            )
            .await?;
        conn.blocks_dal()
            .save_l1_batch_commitment_artifacts(
                L1BatchNumber(1),
                &L1BatchCommitmentArtifacts::default(),
            )
            .await?;

        Ok(())
    }
}

// test that batch is always picked up by prover cluster if prover_cluster proving mode is enabled
#[tokio::test]
async fn test_picking_batch_with_prover_cluster_mode() {
    let ctx = TestContext::new().await.init().await;

    let processor = ctx.processor(ProvingMode::ProverCluster).await;

    let batch = processor
        .lock_batch_for_proving(ctx.config.proof_generation_timeout)
        .await
        .unwrap();
    assert_eq!(batch, Some(L1BatchNumber(1)));
}
