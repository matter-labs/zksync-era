use zksync_commitment_generator::CommitmentGenerator;

use crate::{
    implementations::resources::{healthcheck::AppHealthCheckResource, pools::MasterPoolResource},
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

pub struct CommitmentGeneratorLayer;

#[async_trait::async_trait]
impl WiringLayer for CommitmentGeneratorLayer {
    fn layer_name(&self) -> &'static str {
        "commitment_generator_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool_resource = context.get_resource::<MasterPoolResource>().await?;
        let main_pool = pool_resource.get().await.unwrap();

        let commitment_generator = CommitmentGenerator::new(main_pool);

        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
        app_health.insert_component(commitment_generator.health_check());

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
