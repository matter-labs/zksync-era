//! Tests for the `snapshots` Web3 namespace.

use std::collections::HashSet;

use zksync_types::snapshots::SnapshotVersion;
use zksync_web3_decl::namespaces::SnapshotsNamespaceClient;

use super::*;

#[derive(Debug)]
struct SnapshotBasicsTest {
    chunk_ids: HashSet<u64>,
}

impl SnapshotBasicsTest {
    const CHUNK_COUNT: u64 = 5;

    fn new(chunk_ids: impl IntoIterator<Item = u64>) -> Self {
        let chunk_ids: HashSet<_> = chunk_ids.into_iter().collect();
        assert!(chunk_ids.iter().all(|&id| id < Self::CHUNK_COUNT));
        Self { chunk_ids }
    }

    fn is_complete_snapshot(&self) -> bool {
        self.chunk_ids == HashSet::from_iter(0..Self::CHUNK_COUNT)
    }
}

impl TestInit for SnapshotBasicsTest {}

#[async_trait]
impl HttpTest for SnapshotBasicsTest {
    async fn test(
        &self,
        client: &DynClient<L2>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let mut storage = pool.connection().await.unwrap();
        store_l2_block(
            &mut storage,
            L2BlockNumber(1),
            &[mock_execute_transaction(create_l2_transaction(1, 2).into())],
        )
        .await?;
        seal_l1_batch(&mut storage, L1BatchNumber(1)).await?;
        storage
            .snapshots_dal()
            .add_snapshot(
                SnapshotVersion::Version0,
                L1BatchNumber(1),
                Self::CHUNK_COUNT,
                "file:///factory_deps",
            )
            .await?;

        for &chunk_id in &self.chunk_ids {
            let path = format!("file:///storage_logs/chunk{chunk_id}");
            storage
                .snapshots_dal()
                .add_storage_logs_filepath_for_snapshot(L1BatchNumber(1), chunk_id, &path)
                .await?;
        }

        let all_snapshots = client.get_all_snapshots().await?;
        if self.is_complete_snapshot() {
            assert_eq!(all_snapshots.snapshots_l1_batch_numbers, [L1BatchNumber(1)]);
        } else {
            assert_eq!(all_snapshots.snapshots_l1_batch_numbers, []);
        }

        let snapshot_header = client
            .get_snapshot_by_l1_batch_number(L1BatchNumber(1))
            .await?;
        let snapshot_header = if self.is_complete_snapshot() {
            snapshot_header.context("no snapshot for L1 batch #1")?
        } else {
            assert!(snapshot_header.is_none());
            return Ok(());
        };

        assert_eq!(snapshot_header.l1_batch_number, L1BatchNumber(1));
        assert_eq!(snapshot_header.l2_block_number, L2BlockNumber(1));
        assert_eq!(
            snapshot_header.factory_deps_filepath,
            "file:///factory_deps"
        );

        assert_eq!(
            snapshot_header.storage_logs_chunks.len(),
            self.chunk_ids.len()
        );
        for chunk in &snapshot_header.storage_logs_chunks {
            assert!(self.chunk_ids.contains(&chunk.chunk_id));
            assert!(chunk.filepath.starts_with("file:///storage_logs/"));
        }
        Ok(())
    }
}

#[tokio::test]
async fn snapshot_without_chunks() {
    test_http_server(SnapshotBasicsTest::new([])).await;
}

#[tokio::test]
async fn snapshot_with_some_chunks() {
    test_http_server(SnapshotBasicsTest::new([0, 2, 4])).await;
}

#[tokio::test]
async fn snapshot_with_all_chunks() {
    test_http_server(SnapshotBasicsTest::new(0..SnapshotBasicsTest::CHUNK_COUNT)).await;
}
