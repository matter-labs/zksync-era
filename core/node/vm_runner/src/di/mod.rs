//! Dependency injection for VM runner components.

use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
};

pub use self::{
    bwip::BasicWitnessInputProducerLayer, playground::VmPlaygroundLayer,
    protective_reads::ProtectiveReadsWriterLayer,
};
use crate::{ConcurrentOutputHandlerFactoryTask, StorageSyncTask, VmRunnerIo};

mod bwip;
mod playground;
mod protective_reads;

#[async_trait::async_trait]
impl<Io: VmRunnerIo> Task for StorageSyncTask<Io> {
    fn id(&self) -> TaskId {
        format!("vm_runner/{}/storage_sync", self.io().name()).into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl<Io: VmRunnerIo> Task for ConcurrentOutputHandlerFactoryTask<Io> {
    fn id(&self) -> TaskId {
        format!("vm_runner/{}/output_handler", self.io().name()).into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
