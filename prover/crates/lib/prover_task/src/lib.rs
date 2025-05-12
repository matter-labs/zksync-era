use std::time::Duration;

use anyhow::Context;
use tracing::Instrument;

/// Task trait to be run in ProverJobMonitor.
#[async_trait::async_trait]
pub trait Task {
    async fn invoke(&self) -> anyhow::Result<()>;
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
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Started Task {} with run interval: {:?}",
            self.name,
            self.interval
        );

        let mut interval = tokio::time::interval(self.interval);
        while !*stop_receiver.borrow_and_update() {
            interval.tick().await;
            self.job
                .invoke()
                .instrument(tracing::info_span!("run", service_name = %self.name))
                .await
                .context("failed to invoke task")?;
        }
        tracing::info!("Stop request received; Task {} is shut down", self.name);
        Ok(())
    }
}

/// Wrapper on a vector of task. Makes adding/spawning tasks and sharing resources ergonomic.
#[derive(Default)]
pub struct TaskRunner {
    tasks: Vec<PeriodicTask>,
}

impl TaskRunner {
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
                let receiver = stop_receiver.clone();
                tokio::spawn(async move { task.run(receiver).await })
            })
            .collect()
    }
}
