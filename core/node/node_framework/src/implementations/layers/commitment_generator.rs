use std::num::NonZero;

use zksync_commitment_generator::CommitmentGenerator;
use zksync_types::commitment::L1BatchCommitmentMode;

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for l1 batches commitment generation
///
/// Responsible for initialization and running [`CommitmentGenerator`].
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds tasks
///
/// - `CommitmentGeneratorTask`
#[derive(Debug)]
pub struct CommitmentGeneratorLayer {
    mode: L1BatchCommitmentMode,
    max_parallelism: Option<NonZero<u32>>,
}

impl CommitmentGeneratorLayer {
    pub fn new(mode: L1BatchCommitmentMode) -> Self {
        Self {
            mode,
            max_parallelism: None,
        }
    }

    pub fn with_max_parallelism(mut self, max_parallelism: Option<NonZero<u32>>) -> Self {
        self.max_parallelism = max_parallelism;
        self
    }
}

#[async_trait::async_trait]
impl WiringLayer for CommitmentGeneratorLayer {
    fn layer_name(&self) -> &'static str {
        "commitment_generator_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool_resource = context.get_resource::<PoolResource<MasterPool>>()?;

        let pool_size = self
            .max_parallelism
            .unwrap_or(CommitmentGenerator::default_parallelism())
            .get();
        let main_pool = pool_resource.get_custom(pool_size).await?;

        let mut commitment_generator = CommitmentGenerator::new(main_pool, self.mode);
        if let Some(max_parallelism) = self.max_parallelism {
            commitment_generator.set_max_parallelism(max_parallelism);
        }

        let AppHealthCheckResource(app_health) = context.get_resource_or_default();
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
    fn id(&self) -> TaskId {
        "commitment_generator".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.commitment_generator.run(stop_receiver.0).await
    }
}
