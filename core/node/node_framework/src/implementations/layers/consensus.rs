use zksync_concurrency::ctx;
use zksync_core::consensus::{self, MainNodeConfig};
use zksync_dal::{ConnectionPool, Core};

use crate::{
    implementations::resources::pools::MasterPoolResource,
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug, Copy, Clone)]
pub enum Mode {
    Main,
    External,
}

#[derive(Debug)]
pub struct ConsensusLayer {
    mode: Mode,
    config: Option<consensus::Config>,
    secrets: Option<consensus::Secrets>,
}

#[async_trait::async_trait]
impl WiringLayer for ConsensusLayer {
    fn layer_name(&self) -> &'static str {
        "consensus_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context
            .get_resource::<MasterPoolResource>()
            .await?
            .get()
            .await?;

        match self.mode {
            Mode::Main => {
                let config = self.config.ok_or_else(|| {
                    WiringError::Configuration("Missing public consensus config".to_string())
                })?;
                let secrets = self.secrets.ok_or_else(|| {
                    WiringError::Configuration("Missing private consensus config".to_string())
                })?;

                let main_node_config = config.main_node(&secrets)?;

                let task = MainNodeConsensusTask {
                    config: main_node_config,
                    pool,
                };
                context.add_task(Box::new(task));
            }
            Mode::External => {
                unimplemented!("External consensus mode is not implemented yet");
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct MainNodeConsensusTask {
    config: MainNodeConfig,
    pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl Task for MainNodeConsensusTask {
    fn name(&self) -> &'static str {
        "consensus"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let root_ctx = ctx::root();
        zksync_core::consensus::run_main_node(&root_ctx, self.config, self.pool, stop_receiver.0)
            .await
    }
}
