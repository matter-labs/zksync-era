use zksync_vm_runner::{ConcurrentOutputHandlerFactoryTask, StorageSyncTask, VmRunnerIo};

use crate::{
    service::StopReceiver,
    task::{Task, TaskId},
};

pub mod bwip;
pub mod protective_reads;

#[async_trait::async_trait]
impl<Io: VmRunnerIo> Task for StorageSyncTask<Io> {
    fn id(&self) -> TaskId {
        format!("vm_runner/{}/storage_sync", self.io().name()).into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        StorageSyncTask::run(*self, stop_receiver.0.clone()).await?;
        stop_receiver.0.changed().await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<Io: VmRunnerIo> Task for ConcurrentOutputHandlerFactoryTask<Io> {
    fn id(&self) -> TaskId {
        format!("vm_runner/{}/output_handler", self.io().name()).into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        ConcurrentOutputHandlerFactoryTask::run(*self, stop_receiver.0.clone()).await?;
        stop_receiver.0.changed().await?;
        Ok(())
    }
}
