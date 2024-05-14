use zksync_commitment_generator::{
    commitment_post_processor::{
        CommitmentPostProcessor, RollupCommitmentPostProcessor, ValidiumCommitmentPostProcessor,
    },
    input_generation::{InputGenerator, RollupInputGenerator, ValidiumInputGenerator},
    CommitmentGenerator,
};
use zksync_config::configs::chain::L1BatchCommitDataGeneratorMode;

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

pub struct CommitmentGeneratorLayer {
    input_generator: Box<dyn InputGenerator>,
    commitment_post_processor: Box<dyn CommitmentPostProcessor>,
}

impl CommitmentGeneratorLayer {
    pub fn new(commit_data_generator_mode: L1BatchCommitDataGeneratorMode) -> Self {
        let (input_generator, commitment_post_processor): (
            Box<dyn InputGenerator>,
            Box<dyn CommitmentPostProcessor>,
        ) = if commit_data_generator_mode == L1BatchCommitDataGeneratorMode::Validium {
            (
                Box::new(ValidiumInputGenerator),
                Box::new(ValidiumCommitmentPostProcessor),
            )
        } else {
            (
                Box::new(RollupInputGenerator),
                Box::new(RollupCommitmentPostProcessor),
            )
        };

        Self {
            input_generator,
            commitment_post_processor,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for CommitmentGeneratorLayer {
    fn layer_name(&self) -> &'static str {
        "commitment_generator_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let main_pool = pool_resource.get().await.unwrap();

        let commitment_generator = CommitmentGenerator::new(
            main_pool,
            self.input_generator,
            self.commitment_post_processor,
        );

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
