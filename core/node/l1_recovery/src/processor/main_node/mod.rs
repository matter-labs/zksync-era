use tempfile::TempDir;
use zksync_dal::{ConnectionPool, Core};

use crate::{
    l1_fetcher::l1_fetcher::{L1Fetcher, L1FetcherConfig},
    processor::tree::TreeProcessor,
};

#[derive(Debug)]
pub struct SnapshotsApplierTask {
    connection_pool: ConnectionPool<Core>,
    l1_fetcher: L1Fetcher,
    tree: TreeProcessor,
}

impl SnapshotsApplierTask {
    pub async fn new(connection_pool: ConnectionPool<Core>) -> Self {
        let config = L1FetcherConfig {
            http_url: "https://ethereum-sepolia-rpc.publicnode.com".to_string(),
            blobs_url: "https://api.sepolia.blobscan.com/blobs/".to_string(),
            block_step: 1000,
        };
        let temp_dir = TempDir::new().unwrap().into_path().join("db");
        let tree = TreeProcessor::new(temp_dir).await.unwrap();
        let l1_fetcher = L1Fetcher::new(config, None).unwrap();
        Self {
            connection_pool,
            l1_fetcher,
            tree,
        }
    }
    //
    // pub async fn run(mut self) -> anyhow::Result<()> {
    //     let transaction = &mut self.connection_pool.connection().await.unwrap();
    //     let initial_entries = self.tree.process_genesis_state(sepolia_initial_state_path()).await?;
    //
    //     let blocks = self.l1_fetcher.get_blocks_to_process(U64::from(4800000), U64::from(4820508)).await;
    //     for block in blocks {
    //
    //         let factory_deps = block.factory_deps.clone();
    //         let tree_entries = self.tree.process_one_block(block).await;
    //     }
    //     Ok(())
    // }
}
