use zksync_config::configs::da_client::eigen_da::EigenDAConfig;

use crate::{
    implementations::resources::{
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for eigenda server.
#[derive(Debug)]
pub struct EigenDAProxyLayer {
    eigenda_config: EigenDAConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub object_store: ObjectStoreResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub task: EigenDAProxyTask,
}

impl EigenDAProxyLayer {
    pub fn new(eigenda_config: EigenDAConfig) -> Self {
        Self { eigenda_config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EigenDAProxyLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eigenda_proxy_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let task = EigenDAProxyTask {};

        Ok(Output { task })
    }
}

#[derive(Debug)]
pub struct EigenDAProxyTask {}

#[async_trait::async_trait]
impl Task for EigenDAProxyTask {
    fn id(&self) -> TaskId {
        "eigenda_proxy".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        zksync_eigenda_proxy::run_server(stop_receiver.0).await
    }
}
