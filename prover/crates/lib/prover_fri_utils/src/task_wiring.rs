use std::time::Duration;

use anyhow::Context;
use tracing::Instrument;
use zksync_prover_dal::{Connection, ConnectionPool, Prover};

/// Trait for types that can provide a database connection.
#[async_trait::async_trait]
pub trait ProvideConnection: Send + Sync {
    /// Asynchronously gets a new connection.
    async fn get(&self) -> anyhow::Result<Connection<Prover>>;
}

/// Implement the connection provider trait for the ConnectionPool.
#[async_trait::async_trait]
impl ProvideConnection for ConnectionPool<Prover> {
    async fn get(&self) -> anyhow::Result<Connection<Prover>> {
        self.connection()
            .await
            .context("Failed to get connection from pool provider")
    }
}

/// Task trait to be run in ProverJobMonitor.
#[async_trait::async_trait]
pub trait Task {
    async fn invoke(
        &self,
        connection_provider: Option<&(dyn ProvideConnection + Send + Sync)>,
    ) -> anyhow::Result<()>;
}

/// Wrapper for Task with a periodic interface. Holds information about the task and provides DB connectivity.
struct PeriodicTask {
    job: Box<dyn Task + Send + Sync>,
    name: String,
    interval: Duration,
}

impl PeriodicTask {
    async fn run(
        &self,
        mut stop_receiver: tokio::sync::watch::Receiver<bool>,
        connection_pool: Option<ConnectionPool<Prover>>,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Started Task {} with run interval: {:?}",
            self.name,
            self.interval
        );

        let mut interval = tokio::time::interval(self.interval);
        let connection_getter = connection_pool.as_ref().map(|pool| pool as _);
        while !*stop_receiver.borrow_and_update() {
            interval.tick().await;
            self.job
                .invoke(connection_getter)
                .instrument(tracing::info_span!("run", service_name = %self.name))
                .await
                .context("failed to invoke task")?;
        }
        tracing::info!("Stop signal received; Task {} is shut down", self.name);
        Ok(())
    }
}

/// Wrapper on a vector of task. Makes adding/spawning tasks and sharing resources ergonomic.
#[derive(Default)]
pub struct TaskRunner {
    pool: Option<ConnectionPool<Prover>>,
    tasks: Vec<PeriodicTask>,
}

impl TaskRunner {
    pub fn with_pool(&mut self, pool: ConnectionPool<Prover>) -> &mut Self {
        self.pool = Some(pool);
        self
    }

    pub fn add<T: Task + Send + Sync + 'static>(&mut self, name: &str, interval: Duration, job: T) {
        self.tasks.push(PeriodicTask {
            name: name.into(),
            interval,
            job: Box::new(job),
        });
    }

    pub fn spawn(
        self,
        stop_receiver: tokio::sync::watch::Receiver<bool>,
    ) -> Vec<tokio::task::JoinHandle<anyhow::Result<()>>> {
        self.tasks
            .into_iter()
            .map(|task| {
                let pool = self.pool.clone();
                let receiver = stop_receiver.clone();
                tokio::spawn(async move { task.run(receiver, pool).await })
            })
            .collect()
    }
}
