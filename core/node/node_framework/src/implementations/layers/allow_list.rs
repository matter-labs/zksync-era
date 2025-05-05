use std::collections::HashSet;

use async_trait::async_trait;
use zksync_config::configs::chain::DeploymentAllowlist;
use zksync_node_api_server::tx_sender::whitelist::AllowListTask;
use zksync_vm_executor::whitelist::SharedAllowList;

use crate::{
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    IntoContext, Resource,
};

/// Wiring layer for [`AllowListTask`] that controls deployment allow list .
pub struct DeploymentAllowListLayer {
    pub deployment_allowlist: DeploymentAllowlist,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub shared_allow_list: SharedAllowList,
    #[context(task)]
    pub allow_list_task: Option<AllowListTask>,
}

#[async_trait]
impl WiringLayer for DeploymentAllowListLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "deployment_allowlist_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
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

        Ok(Output {
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
        "allowlist_task".into()
    }

    fn kind(&self) -> TaskKind {
        TaskKind::Task
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
