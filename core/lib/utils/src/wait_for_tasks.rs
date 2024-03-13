use std::time::Duration;

use futures::future;
use tokio::task::JoinHandle;

use crate::panic_extractor::try_extract_panic_message;

#[must_use = "remaining tasks should be completed"]
#[derive(Debug)]
pub struct ManagedTasks {
    task_handles: Vec<JoinHandle<anyhow::Result<()>>>,
    tasks_allowed_to_finish: bool,
}

impl ManagedTasks {
    pub fn new(task_handles: Vec<JoinHandle<anyhow::Result<()>>>) -> Self {
        Self {
            task_handles,
            tasks_allowed_to_finish: false,
        }
    }

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
                    vlog::capture_message(err, vlog::AlertLevel::Warning);
                }
            }
            Ok(Err(err)) => {
                let err =
                    format!("One of the tokio actors unexpectedly finished with error: {err:#}");
                tracing::error!("{err}");
                vlog::capture_message(&err, vlog::AlertLevel::Warning);
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

// FIXME: test
