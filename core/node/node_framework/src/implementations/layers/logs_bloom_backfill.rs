use zksync_logs_bloom_backfill::LogsBloomBackfill;

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for ethereum watcher
///
/// Responsible for initializing and running of [`LogsBloomBackfill`] task, that backfills `logsBloom` for old blocks.
#[derive(Debug)]
pub struct LogsBloomBackfillLayer;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub logs_bloom_backfill: LogsBloomBackfill,
}

#[async_trait::async_trait]
impl WiringLayer for LogsBloomBackfillLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "logs_bloom_backfill_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_pool = input.master_pool.get_singleton().await.unwrap();
        let logs_bloom_backfill = LogsBloomBackfill::new(main_pool);
        Ok(Output {
            logs_bloom_backfill,
        })
    }
}

#[async_trait::async_trait]
impl Task for LogsBloomBackfill {
    fn kind(&self) -> TaskKind {
        TaskKind::OneshotTask
    }

    fn id(&self) -> TaskId {
        "logs_bloom_backfill".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
