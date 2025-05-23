use zksync_node_framework::{IntoContext, StopReceiver, Task, TaskId, WiringError, WiringLayer};
use zksync_types::L1BatchNumber;

use crate::{node::BlockReverterResource, BlockReverter};

/// Layer that unconditionally reverts to a specific L1 batch.
#[derive(Debug)]
pub struct UnconditionalRevertLayer {
    l1_batch: L1BatchNumber,
}

impl UnconditionalRevertLayer {
    pub fn new(l1_batch: L1BatchNumber) -> Self {
        Self { l1_batch }
    }
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    revert_task: UnconditionalRevertTask,
}

#[async_trait::async_trait]
impl WiringLayer for UnconditionalRevertLayer {
    type Input = BlockReverterResource;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "unconditional_block_revert_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let reverter = input.0.take().ok_or(WiringError::Configuration(
            "BlockReverterResource is taken".into(),
        ))?;
        Ok(Output {
            revert_task: UnconditionalRevertTask {
                reverter,
                l1_batch: self.l1_batch,
            },
        })
    }
}

#[derive(Debug)]
struct UnconditionalRevertTask {
    reverter: BlockReverter,
    l1_batch: L1BatchNumber,
}

#[async_trait::async_trait]
impl Task for UnconditionalRevertTask {
    fn id(&self) -> TaskId {
        "unconditional_revert_task".into()
    }

    // The task is intentionally not responsive to stop requests; the revert is supposed to be atomic.
    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.reverter.roll_back(self.l1_batch).await
    }
}
