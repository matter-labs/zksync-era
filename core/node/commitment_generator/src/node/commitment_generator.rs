use std::{num::NonZero, sync::Arc};

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

use crate::CommitmentGenerator;

/// Wiring layer for l1 batches commitment generation
///
/// Responsible for initialization and running [`CommitmentGenerator`].
#[derive(Debug, Default)]
pub struct CommitmentGeneratorLayer {
    max_parallelism: Option<NonZero<u32>>,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    commitment_generator: CommitmentGenerator,
}

impl CommitmentGeneratorLayer {
    pub fn with_max_parallelism(mut self, max_parallelism: Option<NonZero<u32>>) -> Self {
        self.max_parallelism = max_parallelism;
        self
    }
}

#[async_trait::async_trait]
impl WiringLayer for CommitmentGeneratorLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "commitment_generator_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool_size = self
            .max_parallelism
            .unwrap_or(CommitmentGenerator::default_parallelism())
            .get();
        let main_pool = input.master_pool.get_custom(pool_size).await?;

        let mut commitment_generator = CommitmentGenerator::new(main_pool);
        if let Some(max_parallelism) = self.max_parallelism {
            commitment_generator.set_max_parallelism(max_parallelism);
        }

        input
            .app_health
            .insert_component(commitment_generator.health_check())
            .map_err(WiringError::internal)?;

        Ok(Output {
            commitment_generator,
        })
    }
}

#[async_trait::async_trait]
impl Task for CommitmentGenerator {
    fn id(&self) -> TaskId {
        "commitment_generator".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
