use std::sync::Arc;

use serde::Serialize;
use zksync_config::{configs::api::HealthCheckConfig, CapturedParams};
use zksync_health_check::{AppHealthCheck, CheckHealth, Health, HealthStatus};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_metrics::metadata::{GitMetadata, RustMetadata, GIT_METRICS, RUST_METRICS};

use crate::healthcheck::HealthCheckHandle;

/// Full metadata of the compiled binary.
#[derive(Debug, Serialize)]
pub struct BinMetadata {
    pub rust: &'static RustMetadata,
    pub git: &'static GitMetadata,
}

/// Wiring layer for health check server
///
/// Expects other layers to insert different components' health checks
/// into [`AppHealthCheck`] aggregating heath using [`AppHealthCheckResource`].
/// The added task spawns a health check server that only exposes the state provided by other tasks.
#[derive(Debug)]
pub struct HealthCheckLayer {
    config: HealthCheckConfig,
    config_params: Option<CapturedParams>,
}

impl HealthCheckLayer {
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            config,
            config_params: None,
        }
    }

    #[must_use]
    pub fn with_config_params(mut self, params: CapturedParams) -> Self {
        self.config_params = Some(params);
        self
    }
}

#[derive(Debug, FromContext)]
pub struct Input {
    #[context(default)]
    app_health_check: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    health_check_task: HealthCheckTask,
}

#[async_trait::async_trait]
impl WiringLayer for HealthCheckLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "healthcheck_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let app_health_check = input.app_health_check;
        app_health_check.override_limits(self.config.slow_time_limit, self.config.hard_time_limit);

        if let (true, Some(params)) = (self.config.expose_config, self.config_params) {
            tracing::info!(
                params.len = params.len(),
                "Exposing config params as part of healthcheck server"
            );
            let config_health = ConfigHealth(params);
            app_health_check
                .insert_custom_component(Arc::new(config_health))
                .map_err(WiringError::internal)?;
        }

        let health_check_task = HealthCheckTask {
            config: self.config,
            app_health_check,
        };

        Ok(Output { health_check_task })
    }
}

#[derive(Debug)]
pub struct HealthCheckTask {
    config: HealthCheckConfig,
    app_health_check: Arc<AppHealthCheck>,
}

#[async_trait::async_trait]
impl Task for HealthCheckTask {
    fn kind(&self) -> TaskKind {
        TaskKind::UnconstrainedTask
    }

    fn id(&self) -> TaskId {
        "healthcheck_server".into()
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.app_health_check.set_details(BinMetadata {
            rust: RUST_METRICS.initialize(),
            git: GIT_METRICS.initialize(),
        });
        let handle = HealthCheckHandle::spawn_server(self.config.port, self.app_health_check);
        stop_receiver.0.changed().await?;
        handle.stop().await;

        Ok(())
    }
}

#[derive(Debug)]
struct ConfigHealth(CapturedParams);

#[async_trait::async_trait]
impl CheckHealth for ConfigHealth {
    fn name(&self) -> &'static str {
        "config"
    }

    async fn check_health(&self) -> Health {
        Health::from(HealthStatus::Ready).with_details(&self.0)
    }
}
