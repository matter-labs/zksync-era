use std::collections::HashSet;

use async_trait::async_trait;
use zksync_config::configs::api::DeploymentAllowlist;
use zksync_node_api_server::tx_sender::{
    master_pool_sink::MasterPoolSink,
    whitelist::{AllowListTask, WhitelistedDeployPoolSink},
};
use zksync_vm_executor::whitelist::{DeploymentTxFilter, SharedAllowList};

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        web3_api::TxSinkResource,
    },
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext, Resource,
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
    pub shared_allow_list: SharedAllowList,
    #[context(task)]
    pub allow_list_task: Option<AllowListTask>,
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

        let (task, shared_list) = match self.deployment_allowlist {
            DeploymentAllowlist::Dynamic(allow_list) => {
                let allow_list_task = AllowListTask::from_config(allow_list);
                let shared = allow_list_task.shared();
                (Some(allow_list_task), shared)
            }
            DeploymentAllowlist::Static(addresses) => (
                None,
                SharedAllowList::new(HashSet::from_iter(addresses.into_iter())),
            ),
        };

        let tx_sink = WhitelistedDeployPoolSink::new(
            master_pool_sink,
            DeploymentTxFilter::new(shared_list.clone()),
        )
        .into();

        Ok(Output {
            tx_sink,
            shared_allow_list: shared_list,
            allow_list_task: task,
        })
    }
}

impl Resource for SharedAllowList {
    fn name() -> String {
        "shared_allow_list".to_string()
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
