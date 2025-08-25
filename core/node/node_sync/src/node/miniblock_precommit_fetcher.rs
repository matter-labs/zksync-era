use std::sync::Arc;

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_web3_decl::client::{DynClient, L2};

use crate::miniblock_precommit_fetcher::MiniblockPrecommitFetcher;

#[derive(Debug, FromContext)]
pub struct Input {
    pool: PoolResource<MasterPool>,
    main_node_client: Box<DynClient<L2>>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    fetcher: MiniblockPrecommitFetcher,
}

/// Wiring layer for `MiniblockPrecommitFetcher`, part of the external node.
#[derive(Debug)]
pub struct MiniblockPrecommitFetcherLayer;

#[async_trait::async_trait]
impl WiringLayer for MiniblockPrecommitFetcherLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "miniblock_precommit_fetcher_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let Input {
            pool,
            main_node_client,
            app_health,
        } = input;

        let fetcher = MiniblockPrecommitFetcher::new(Box::new(main_node_client), pool.get().await?);

        // Insert healthcheck
        app_health
            .insert_component(fetcher.health_check())
            .map_err(WiringError::internal)?;

        Ok(Output { fetcher })
    }
}

#[async_trait::async_trait]
impl Task for MiniblockPrecommitFetcher {
    fn id(&self) -> TaskId {
        "miniblock_precommit_fetcher".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}
