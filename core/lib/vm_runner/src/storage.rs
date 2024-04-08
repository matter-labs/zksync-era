use std::fmt::Debug;
use std::marker::PhantomData;

use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core};
use zksync_state::{PgOrRocksdbStorage, ReadStorageFactory};
use zksync_types::{block::MiniblockExecutionData, L1BatchNumber};

struct BatchData {
    /// List of miniblocks and corresponding transactions that were executed within batch.
    pub(crate) miniblocks: Vec<MiniblockExecutionData>,
}

#[async_trait]
trait VmRunnerStorageLoader: Debug + Send + Sync + 'static {
    /// Loads the next L1 batch data from the database.
    ///
    /// # Errors
    ///
    /// Propagates DB errors. Also returns an error if environment doesn't correspond to a pending L1 batch.
    async fn load_next_batch(conn: Connection<'_, Core>) -> anyhow::Result<BatchData>;
}

#[derive(Debug)]
struct VmRunnerStorage<L: VmRunnerStorageLoader> {
    pool: ConnectionPool<Core>,
    _marker: PhantomData<L>,
}

impl<L: VmRunnerStorageLoader> VmRunnerStorage<L> {
    pub(crate) async fn load_next_batch(&self) -> anyhow::Result<BatchData> {
        let conn = self.pool.connection().await?;
        L::load_next_batch(conn).await
    }
}

#[async_trait]
impl<L: VmRunnerStorageLoader> ReadStorageFactory for VmRunnerStorage<L> {
    async fn access_storage(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        todo!()
    }
}
