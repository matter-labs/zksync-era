// TODO: Migrate contract-verifier and provers to node_framework and get rid of this crate
use std::time::Duration;

use futures::future;
use tokio::task::JoinHandle;
use zksync_utils::panic_extractor::try_extract_panic_message;

/// Container for fallible Tokio tasks with ability to track their shutdown.
///
/// The intended usage is [first waiting for a single task](Self::wait_single()) (perhaps in a `select!`
/// with other futures, e.g. a termination signal listener), and then [completing](Self::complete())
/// the remaining tasks.
#[must_use = "Tasks should be `complete()`d"]
#[derive(Debug)]
pub struct ManagedTasks {
    task_handles: Vec<JoinHandle<anyhow::Result<()>>>,
    tasks_allowed_to_finish: bool,
}

impl ManagedTasks {
    /// Wraps the specified list of Tokio tasks.
    pub fn new(task_handles: Vec<JoinHandle<anyhow::Result<()>>>) -> Self {
        Self {
            task_handles,
            tasks_allowed_to_finish: false,
        }
    }

    /// Specifies that the wrapped tasks can finish (by default, they are expected to run indefinitely).
    /// This influences logging when a task finishes.
    pub fn allow_tasks_to_finish(mut self) -> Self {
        self.tasks_allowed_to_finish = true;
        self
    }

    /// Waits until a single managed task terminates, no matter the outcome.
    pub async fn wait_single(&mut self) {
        let (result, completed_index, _) = future::select_all(&mut self.task_handles).await;
        // Remove the completed task so that it doesn't panic when polling tasks in `Self::complete()`.
        self.task_handles.swap_remove(completed_index);

        match result {
            Ok(Ok(())) => {
                if self.tasks_allowed_to_finish {
                    tracing::info!("One of the actors finished its run. Finishing execution.");
                } else {
                    let err =
                        "One of the actors finished its run, while it wasn't expected to do it";
                    tracing::error!("{err}");
                    zksync_vlog::sentry::capture_message(
                        err,
                        zksync_vlog::sentry::AlertLevel::Warning,
                    );
                }
            }
            Ok(Err(err)) => {
                let err =
                    format!("One of the tokio actors unexpectedly finished with error: {err:#}");
                tracing::error!("{err}");
                zksync_vlog::sentry::capture_message(
                    &err,
                    zksync_vlog::sentry::AlertLevel::Warning,
                );
            }
            Err(error) => {
                let panic_message = try_extract_panic_message(error);
                tracing::info!("One of the tokio actors panicked: {panic_message}");
            }
        }
    }

    /// Drives all remaining tasks to completion, logging their errors / panics should they occur.
    pub async fn complete(self, timeout: Duration) {
        if tokio::time::timeout(timeout, self.complete_inner())
            .await
            .is_err()
        {
            tracing::warn!("Failed to terminate actors in {timeout:?}");
        }
    }

    async fn complete_inner(self) {
        let futures = self.task_handles.into_iter().map(|fut| async move {
            match fut.await {
                Ok(Ok(())) => { /* do nothing */ }
                Ok(Err(err)) => {
                    tracing::error!("One of actors returned an error during shutdown: {err:?}");
                }
                Err(err) => tracing::error!("One of actors panicked during shutdown: {err}"),
            }
        });
        future::join_all(futures).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };

    use tokio::sync::watch;

    use super::*;

    #[tokio::test]
    async fn managing_tasks_with_normal_tasks() {
        let (shutdown_sender, shutdown_receiver) = watch::channel(false);
        let counter = Arc::new(AtomicUsize::new(0));
        let tasks = (0..5).map(|_| {
            let mut shutdown_receiver = shutdown_receiver.clone();
            let counter = counter.clone();
            tokio::spawn(async move {
                shutdown_receiver.changed().await.unwrap();
                counter.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
        });
        let mut tasks = ManagedTasks::new(tasks.collect());
        tokio::select! {
            () = tasks.wait_single() => {
                panic!("Tasks should not finish in this test");
            }
            () = tokio::time::sleep(Duration::from_millis(50)) => {
                // Emulate shutdown after a delay.
            }
        }
        shutdown_sender.send_replace(true);
        tasks.complete(Duration::from_secs(1)).await;
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }

    #[derive(Debug)]
    enum TaskTermination {
        Ok,
        Error,
        Panic,
    }

    impl TaskTermination {
        const ALL: [Self; 3] = [Self::Ok, Self::Error, Self::Panic];
    }

    #[tokio::test]
    async fn managing_tasks_with_terminating_task() {
        for termination in TaskTermination::ALL {
            println!("Testing termination outcome {termination:?}");

            let is_finished = Arc::new(AtomicBool::new(false));
            let (shutdown_sender, mut shutdown_receiver) = watch::channel(false);
            let is_finished_for_task = is_finished.clone();
            let mut tasks = ManagedTasks::new(vec![
                tokio::spawn(async move {
                    tokio::task::yield_now().await;
                    match termination {
                        TaskTermination::Ok => Ok(()),
                        TaskTermination::Error => Err(anyhow::anyhow!("error")),
                        TaskTermination::Panic => panic!("oops"),
                    }
                }),
                tokio::spawn(async move {
                    shutdown_receiver.changed().await.unwrap();
                    is_finished_for_task.store(true, Ordering::Relaxed);
                    Ok(())
                }),
            ]);
            tasks.wait_single().await;
            shutdown_sender.send_replace(true);
            tasks.complete(Duration::from_secs(1)).await;
            assert!(is_finished.load(Ordering::Relaxed));
        }
    }
}
