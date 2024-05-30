use zksync_commitment_generator::CommitmentGenerator;
use zksync_types::commitment::L1BatchCommitmentMode;

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct CommitmentGeneratorLayer {
    mode: L1BatchCommitmentMode,
}

impl CommitmentGeneratorLayer {
    pub fn new(mode: L1BatchCommitmentMode) -> Self {
        Self { mode }
    }
}

#[async_trait::async_trait]
impl WiringLayer for CommitmentGeneratorLayer {
    fn layer_name(&self) -> &'static str {
        "commitment_generator_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let pool_size = CommitmentGenerator::default_parallelism().get();
        let main_pool = pool_resource.get_custom(pool_size).await?;

        let commitment_generator = CommitmentGenerator::new(main_pool, self.mode);

        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
        app_health
            .insert_component(commitment_generator.health_check())
            .map_err(WiringError::internal)?;

        context.add_task(Box::new(CommitmentGeneratorTask {
            commitment_generator,
        }));

        Ok(())
    }
}

#[derive(Debug)]
struct CommitmentGeneratorTask {
    commitment_generator: CommitmentGenerator,
}

#[async_trait::async_trait]
impl Task for CommitmentGeneratorTask {
    fn name(&self) -> &'static str {
        "commitment_generator"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.commitment_generator.run(stop_receiver.0).await
    }
}
