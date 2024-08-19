use std::time::Duration;

use anyhow::Context;
use tracing::Instrument;
use zksync_prover_dal::{Connection, ConnectionPool, Prover};

/// Task trait to be run in ProverJobMonitor.
#[async_trait::async_trait]
pub trait Task {
    async fn invoke(&self, connection: &mut Connection<Prover>) -> anyhow::Result<()>;
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
        connection_pool: ConnectionPool<Prover>,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Started Task {} with run interval: {:?}",
            self.name,
            self.interval
        );

        let mut interval = tokio::time::interval(self.interval);

        while !*stop_receiver.borrow_and_update() {
            interval.tick().await;
            let mut connection = connection_pool
                .connection()
                .await
                .context("failed to get database connection")?;
            self.job
                .invoke(&mut connection)
                .instrument(tracing::info_span!("run", service_name = %self.name))
                .await
                .context("failed to invoke task")?;
        }
        tracing::info!("Stop signal received; Task {} is shut down", self.name);
        Ok(())
    }
}

/// Wrapper on a vector of task. Makes adding/spawning tasks and sharing resources ergonomic.
pub struct TaskRunner {
    pool: ConnectionPool<Prover>,
    tasks: Vec<PeriodicTask>,
}

impl TaskRunner {
    pub fn new(pool: ConnectionPool<Prover>) -> Self {
        Self {
            pool,
            tasks: Vec::new(),
        }
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
