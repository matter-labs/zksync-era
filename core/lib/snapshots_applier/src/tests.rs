mod utils {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use zksync_types::{
        api::en::SyncBlock,
        block::L1BatchHeader,
        commitment::{L1BatchMetaParameters, L1BatchMetadata, L1BatchWithMetadata},
        snapshots::{SnapshotHeader, SnapshotStorageLog},
        AccountTreeId, L1BatchNumber, MiniblockNumber, StorageKey, StorageValue, H160, H256,
    };
    use zksync_web3_decl::jsonrpsee::core::ClientError as RpcError;

    use crate::SnapshotsApplierMainNodeClient;

    #[derive(Debug, Default)]
    pub(crate) struct MockMainNodeClient {
        pub(crate) fetch_l2_block_responses: HashMap<MiniblockNumber, SyncBlock>,
        pub(crate) fetch_newest_snapshot_response: Option<SnapshotHeader>,
    }

    #[async_trait]
    impl SnapshotsApplierMainNodeClient for MockMainNodeClient {
        async fn fetch_l2_block(
            &self,
            number: MiniblockNumber,
        ) -> Result<Option<SyncBlock>, RpcError> {
            if let Some(response) = self.fetch_l2_block_responses.get(&number) {
                Ok(Some((*response).clone()))
            } else {
                Ok(None)
            }
        }

        async fn fetch_newest_snapshot(&self) -> Result<Option<SnapshotHeader>, RpcError> {
            Ok(self.fetch_newest_snapshot_response.clone())
        }
    }

    pub(crate) fn miniblock_metadata(
        miniblock_number: MiniblockNumber,
        l1_batch_number: L1BatchNumber,
        root_hash: H256,
    ) -> SyncBlock {
        SyncBlock {
            number: miniblock_number,
            l1_batch_number,
            last_in_batch: true,
            timestamp: 0,
            l1_gas_price: 0,
            l2_fair_gas_price: 0,
            fair_pubdata_price: None,
            base_system_contracts_hashes: Default::default(),
            operator_address: Default::default(),
            transactions: None,
            virtual_blocks: None,
            hash: Some(root_hash),
            protocol_version: Default::default(),
        }
    }

    pub(crate) fn l1_block_metadata(
        l1_batch_number: L1BatchNumber,
        root_hash: H256,
    ) -> L1BatchWithMetadata {
        L1BatchWithMetadata {
            header: L1BatchHeader::new(
                l1_batch_number,
                0,
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            metadata: L1BatchMetadata {
                root_hash,
                rollup_last_leaf_index: 0,
                merkle_root_hash: Default::default(),
                initial_writes_compressed: vec![],
                repeated_writes_compressed: vec![],
                commitment: Default::default(),
                l2_l1_messages_compressed: vec![],
                l2_l1_merkle_root: Default::default(),
                block_meta_params: L1BatchMetaParameters {
                    zkporter_is_available: false,
                    bootloader_code_hash: Default::default(),
                    default_aa_code_hash: Default::default(),
                },
                aux_data_hash: Default::default(),
                meta_parameters_hash: Default::default(),
                pass_through_data_hash: Default::default(),
                events_queue_commitment: None,
                bootloader_initial_content_commitment: None,
                state_diffs_compressed: vec![],
            },
            factory_deps: vec![],
        }
    }

    pub(crate) fn random_storage_logs(
        l1_batch_number: L1BatchNumber,
        chunk_id: u64,
        logs_per_chunk: u64,
    ) -> Vec<SnapshotStorageLog> {
        (0..logs_per_chunk)
            .map(|x| SnapshotStorageLog {
                key: StorageKey::new(
                    AccountTreeId::from_fixed_bytes(H160::random().to_fixed_bytes()),
                    H256::random(),
                ),
                value: StorageValue::random(),
                l1_batch_number_of_initial_write: l1_batch_number,
                enumeration_index: x + chunk_id * logs_per_chunk,
            })
            .collect()
    }
}

mod snapshots_applier_tests {
    use zksync_dal::ConnectionPool;
    use zksync_object_store::ObjectStoreFactory;
    use zksync_types::{
        snapshots::{
            SnapshotFactoryDependencies, SnapshotFactoryDependency, SnapshotHeader,
            SnapshotRecoveryStatus, SnapshotStorageLog, SnapshotStorageLogsChunk,
            SnapshotStorageLogsChunkMetadata, SnapshotStorageLogsStorageKey,
        },
        Bytes, L1BatchNumber, MiniblockNumber, H256,
    };

    use crate::{
        tests::utils::{
            l1_block_metadata, miniblock_metadata, random_storage_logs, MockMainNodeClient,
        },
        SnapshotsApplier,
    };

    #[tokio::test]
    async fn snapshots_creator_can_successfully_recover_db() {
        let pool = ConnectionPool::test_pool().await;
        let object_store_factory = ObjectStoreFactory::mock();
        let object_store = object_store_factory.create_store().await;
        let mut client = MockMainNodeClient::default();
        let miniblock_number = MiniblockNumber(1234);
        let l1_batch_number = L1BatchNumber(123);
        let l1_root_hash = H256::random();
        let l2_root_hash = H256::random();
        let factory_dep_bytes: Vec<u8> = (0..32).collect();
        let factory_deps = SnapshotFactoryDependencies {
            factory_deps: vec![SnapshotFactoryDependency {
                bytecode: Bytes::from(factory_dep_bytes),
            }],
        };
        object_store
            .put(l1_batch_number, &factory_deps)
            .await
            .unwrap();

        let mut all_snapshot_storage_logs: Vec<SnapshotStorageLog> = vec![];
        for chunk_id in 0..2 {
            let mut chunk_storage_logs = SnapshotStorageLogsChunk {
                storage_logs: random_storage_logs(l1_batch_number, chunk_id, 10),
            };
            let chunk_key = SnapshotStorageLogsStorageKey {
                l1_batch_number,
                chunk_id,
            };
            object_store
                .put(chunk_key, &chunk_storage_logs)
                .await
                .unwrap();
            all_snapshot_storage_logs.append(&mut chunk_storage_logs.storage_logs);
        }

        let snapshot_header = SnapshotHeader {
            l1_batch_number,
            miniblock_number,
            last_l1_batch_with_metadata: l1_block_metadata(l1_batch_number, l1_root_hash),
            storage_logs_chunks: vec![
                SnapshotStorageLogsChunkMetadata {
                    chunk_id: 0,
                    filepath: "file0".to_string(),
                },
                SnapshotStorageLogsChunkMetadata {
                    chunk_id: 1,
                    filepath: "file1".to_string(),
                },
            ],
            factory_deps_filepath: "some_filepath".to_string(),
        };
        client.fetch_newest_snapshot_response = Some(snapshot_header);
        client.fetch_l2_block_responses.insert(
            miniblock_number,
            miniblock_metadata(miniblock_number, l1_batch_number, l2_root_hash),
        );

        SnapshotsApplier::load_snapshot(&pool, &client, &object_store)
            .await
            .unwrap();

        let mut storage = pool.access_storage().await.unwrap();
        let mut recovery_dal = storage.snapshot_recovery_dal();

        let expected_status = SnapshotRecoveryStatus {
            l1_batch_number,
            l1_batch_root_hash: l1_root_hash,
            miniblock_number,
            miniblock_root_hash: l2_root_hash,
            storage_logs_chunks_processed: vec![true, true],
        };

        let current_db_status = recovery_dal.get_applied_snapshot_status().await.unwrap();
        assert_eq!(current_db_status.unwrap(), expected_status);

        let all_initial_writes = storage
            .storage_logs_dedup_dal()
            .dump_all_initial_writes_for_tests()
            .await;

        assert_eq!(all_initial_writes.len(), all_snapshot_storage_logs.len());

        let all_storage_logs = storage
            .storage_logs_dal()
            .dump_all_storage_logs_for_tests()
            .await;

        assert_eq!(all_storage_logs.len(), all_snapshot_storage_logs.len());
    }
}
