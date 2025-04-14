use async_trait::async_trait;
use zksync_config::configs::api::DeploymentAllowlist;
use zksync_node_api_server::tx_sender::{
    master_pool_sink::MasterPoolSink,
    whitelist::{AllowListTask, WhitelistedDeployPoolSink},
};

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        web3_api::TxSinkResource,
    },
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`WhitelistedDeployPoolSink`] that wraps a `MasterPoolSink` and enables allowlist filtering.
pub struct WhitelistedMasterPoolSinkLayer {
    pub deployment_allowlist: DeploymentAllowlist,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub tx_sink: TxSinkResource,
    #[context(task)]
    pub allow_list_task: AllowListTask,
}

#[async_trait]
impl WiringLayer for WhitelistedMasterPoolSinkLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "whitelisted_master_pool_sink_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let master_pool_sink = MasterPoolSink::new(pool);

        let allow_list_task = AllowListTask::from_config(self.deployment_allowlist);

        let tx_sink =
            WhitelistedDeployPoolSink::new(master_pool_sink, allow_list_task.shared()).into();

        Ok(Output {
            tx_sink,
            allow_list_task,
        })
    }
}

#[async_trait]
impl Task for AllowListTask {
    fn id(&self) -> TaskId {
        "api_allowlist_task".into()
    }

    fn kind(&self) -> TaskKind {
        TaskKind::Task
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
