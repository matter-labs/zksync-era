use std::time::Duration;

use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_node_framework::{StopReceiver, Task, TaskId};

use crate::web3::state::SealedL2BlockNumber;

#[derive(Debug)]
pub struct SealedL2BlockUpdaterTask {
    pub number_updater: SealedL2BlockNumber,
    pub pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl Task for SealedL2BlockUpdaterTask {
    fn id(&self) -> TaskId {
        "api_sealed_l2_block_updater_task".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Chosen to be significantly smaller than the interval between L2 blocks, but larger than
        // the latency of getting the latest sealed L2 block number from Postgres. If the API server
        // processes enough requests, information about the latest sealed L2 block will be updated
        // by reporting block difference metrics, so the actual update lag would be much smaller than this value.
        const UPDATE_INTERVAL: Duration = Duration::from_millis(25);

        while !*stop_receiver.0.borrow_and_update() {
            let mut connection = self.pool.connection_tagged("api").await.unwrap();
            let Some(last_sealed_l2_block) =
                connection.blocks_dal().get_sealed_l2_block_number().await?
            else {
                tokio::time::sleep(UPDATE_INTERVAL).await;
                continue;
            };
            drop(connection);

            self.number_updater.update(last_sealed_l2_block);

            if tokio::time::timeout(UPDATE_INTERVAL, stop_receiver.0.changed())
                .await
                .is_ok()
            {
                break;
            }
        }

        Ok(())
    }
}
