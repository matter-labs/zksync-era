//! Tests for filter-related methods in the `eth` namespace.

use zksync_web3_decl::{jsonrpsee::core::ClientError as RpcError, types::FilterChanges};

use super::*;

#[derive(Debug)]
struct BasicFilterChangesTest {
    snapshot_recovery: bool,
}

#[async_trait]
impl HttpTest for BasicFilterChangesTest {
    fn storage_initialization(&self) -> StorageInitialization {
        if self.snapshot_recovery {
            StorageInitialization::empty_recovery()
        } else {
            StorageInitialization::Genesis
        }
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let block_filter_id = client.new_block_filter().await?;
        let tx_filter_id = client.new_pending_transaction_filter().await?;

        // Sleep a little so that the filter timestamp is strictly lesser than the transaction "received at" timestamp.
        tokio::time::sleep(POLL_INTERVAL).await;

        let tx_result = execute_l2_transaction(create_l2_transaction(1, 2));
        let new_tx_hash = tx_result.hash;
        let new_miniblock = store_miniblock(
            &mut pool.access_storage().await?,
            MiniblockNumber(if self.snapshot_recovery {
                StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1
            } else {
                1
            }),
            &[tx_result],
        )
        .await?;

        let block_filter_changes = client.get_filter_changes(block_filter_id).await?;
        assert_matches!(
            block_filter_changes,
            FilterChanges::Hashes(hashes) if hashes == [new_miniblock.hash]
        );
        let block_filter_changes = client.get_filter_changes(block_filter_id).await?;
        assert_matches!(block_filter_changes, FilterChanges::Hashes(hashes) if hashes.is_empty());

        let tx_filter_changes = client.get_filter_changes(tx_filter_id).await?;
        assert_matches!(
            tx_filter_changes,
            FilterChanges::Hashes(hashes) if hashes == [new_tx_hash]
        );
        let tx_filter_changes = client.get_filter_changes(tx_filter_id).await?;
        assert_matches!(tx_filter_changes, FilterChanges::Hashes(hashes) if hashes.is_empty());

        // Check uninstalling the filter.
        let removed = client.uninstall_filter(block_filter_id).await?;
        assert!(removed);
        let removed = client.uninstall_filter(block_filter_id).await?;
        assert!(!removed);

        let err = client
            .get_filter_changes(block_filter_id)
            .await
            .unwrap_err();
        assert_matches!(err, RpcError::Call(err) if err.code() == ErrorCode::InvalidParams.code());
        Ok(())
    }
}

#[tokio::test]
async fn basic_filter_changes() {
    test_http_server(BasicFilterChangesTest {
        snapshot_recovery: false,
    })
    .await;
}

#[tokio::test]
async fn basic_filter_changes_after_snapshot_recovery() {
    test_http_server(BasicFilterChangesTest {
        snapshot_recovery: true,
    })
    .await;
}

#[derive(Debug)]
struct LogFilterChangesTest {
    snapshot_recovery: bool,
}

#[async_trait]
impl HttpTest for LogFilterChangesTest {
    fn storage_initialization(&self) -> StorageInitialization {
        if self.snapshot_recovery {
            StorageInitialization::empty_recovery()
        } else {
            StorageInitialization::Genesis
        }
    }

    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let all_logs_filter_id = client.new_filter(Filter::default()).await?;
        let address_filter = Filter {
            address: Some(Address::repeat_byte(23).into()),
            ..Filter::default()
        };
        let address_filter_id = client.new_filter(address_filter).await?;
        let topics_filter = Filter {
            topics: Some(vec![Some(H256::repeat_byte(42).into())]),
            ..Filter::default()
        };
        let topics_filter_id = client.new_filter(topics_filter).await?;

        let mut storage = pool.access_storage().await?;
        let first_local_miniblock = if self.snapshot_recovery {
            StorageInitialization::SNAPSHOT_RECOVERY_BLOCK + 1
        } else {
            1
        };
        let (_, events) = store_events(&mut storage, first_local_miniblock, 0).await?;
        drop(storage);
        let events: Vec<_> = events.iter().collect();

        let all_logs = client.get_filter_changes(all_logs_filter_id).await?;
        let FilterChanges::Logs(all_logs) = all_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", all_logs);
        };
        assert_logs_match(&all_logs, &events);

        let address_logs = client.get_filter_changes(address_filter_id).await?;
        let FilterChanges::Logs(address_logs) = address_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", address_logs);
        };
        assert_logs_match(&address_logs, &[events[0], events[3]]);

        let topics_logs = client.get_filter_changes(topics_filter_id).await?;
        let FilterChanges::Logs(topics_logs) = topics_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", topics_logs);
        };
        assert_logs_match(&topics_logs, &[events[1], events[3]]);

        let new_all_logs = client.get_filter_changes(all_logs_filter_id).await?;
        let FilterChanges::Hashes(new_all_logs) = new_all_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", new_all_logs);
        };
        assert!(new_all_logs.is_empty());
        Ok(())
    }
}

#[tokio::test]
async fn log_filter_changes() {
    test_http_server(LogFilterChangesTest {
        snapshot_recovery: false,
    })
    .await;
}

#[tokio::test]
async fn log_filter_changes_after_snapshot_recovery() {
    test_http_server(LogFilterChangesTest {
        snapshot_recovery: true,
    })
    .await;
}

#[derive(Debug)]
struct LogFilterChangesWithBlockBoundariesTest;

#[async_trait]
impl HttpTest for LogFilterChangesWithBlockBoundariesTest {
    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()> {
        let lower_bound_filter = Filter {
            from_block: Some(api::BlockNumber::Number(2.into())),
            ..Filter::default()
        };
        let lower_bound_filter_id = client.new_filter(lower_bound_filter).await?;
        let upper_bound_filter = Filter {
            to_block: Some(api::BlockNumber::Number(1.into())),
            ..Filter::default()
        };
        let upper_bound_filter_id = client.new_filter(upper_bound_filter).await?;
        let bounded_filter = Filter {
            from_block: Some(api::BlockNumber::Number(1.into())),
            to_block: Some(api::BlockNumber::Number(1.into())),
            ..Filter::default()
        };
        let bounded_filter_id = client.new_filter(bounded_filter).await?;

        let mut storage = pool.access_storage().await?;
        let (_, events) = store_events(&mut storage, 1, 0).await?;
        drop(storage);
        let events: Vec<_> = events.iter().collect();

        let lower_bound_logs = client.get_filter_changes(lower_bound_filter_id).await?;
        assert_matches!(
            lower_bound_logs,
            FilterChanges::Hashes(hashes) if hashes.is_empty()
        );
        // ^ Since `FilterChanges` is serialized w/o a tag, an empty array will be deserialized
        // as `Hashes(_)` (the first declared variant).

        let upper_bound_logs = client.get_filter_changes(upper_bound_filter_id).await?;
        let FilterChanges::Logs(upper_bound_logs) = upper_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", upper_bound_logs);
        };
        assert_logs_match(&upper_bound_logs, &events);
        let bounded_logs = client.get_filter_changes(bounded_filter_id).await?;
        let FilterChanges::Logs(bounded_logs) = bounded_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", bounded_logs);
        };
        assert_eq!(bounded_logs, upper_bound_logs);

        // Add another miniblock with events to the storage.
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = store_events(&mut storage, 2, 4).await?;
        drop(storage);
        let new_events: Vec<_> = new_events.iter().collect();

        let lower_bound_logs = client.get_filter_changes(lower_bound_filter_id).await?;
        let FilterChanges::Logs(lower_bound_logs) = lower_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", lower_bound_logs);
        };
        assert_logs_match(&lower_bound_logs, &new_events);

        let new_upper_bound_logs = client.get_filter_changes(upper_bound_filter_id).await?;
        assert_matches!(new_upper_bound_logs, FilterChanges::Hashes(hashes) if hashes.is_empty());
        let new_bounded_logs = client.get_filter_changes(bounded_filter_id).await?;
        assert_matches!(new_bounded_logs, FilterChanges::Hashes(hashes) if hashes.is_empty());

        // Add miniblock #3. It should not be picked up by the bounded and upper bound filters,
        // and should be picked up by the lower bound filter.
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = store_events(&mut storage, 3, 8).await?;
        drop(storage);
        let new_events: Vec<_> = new_events.iter().collect();

        let bounded_logs = client.get_filter_changes(bounded_filter_id).await?;
        let FilterChanges::Hashes(bounded_logs) = bounded_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", bounded_logs);
        };
        assert!(bounded_logs.is_empty());

        let upper_bound_logs = client.get_filter_changes(upper_bound_filter_id).await?;
        let FilterChanges::Hashes(upper_bound_logs) = upper_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", upper_bound_logs);
        };
        assert!(upper_bound_logs.is_empty());

        let lower_bound_logs = client.get_filter_changes(lower_bound_filter_id).await?;
        let FilterChanges::Logs(lower_bound_logs) = lower_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", lower_bound_logs);
        };
        assert_logs_match(&lower_bound_logs, &new_events);
        Ok(())
    }
}

#[tokio::test]
async fn log_filter_changes_with_block_boundaries() {
    test_http_server(LogFilterChangesWithBlockBoundariesTest).await;
}
