use tokio::task::JoinHandle;

use crate::{
    task_wiring::{JobPickerTask, JobSaverTask, Task, WorkerPool},
    BackoffAndCancellable, Executor, JobPicker, JobSaver,
};

/// It's preferred to have a minimal amount of jobs in flight at any given time.
/// This ensures that memory usage is minimized, in case of failures, a small amount of jobs is lost and
/// components can apply back pressure to each other in case of misconfiguration.
const CHANNEL_SIZE: usize = 1;

/// The "framework" wrapper that runs the entire machinery.
/// Job Runner is responsible for tying together tasks (picker, executor, saver) and starting them.
#[derive(Debug)]
pub struct JobRunner<E, P, S>
where
    E: Executor,
    P: JobPicker,
    S: JobSaver,
{
    executor: E,
    picker: P,
    saver: S,
    num_workers: usize,
    picker_backoff_and_cancellable: Option<BackoffAndCancellable>,
}

impl<E, P, S> JobRunner<E, P, S>
where
    E: Executor,
    P: JobPicker<ExecutorType = E>,
    S: JobSaver<ExecutorType = E>,
{
    pub fn new(
        executor: E,
        picker: P,
        saver: S,
        num_workers: usize,
        picker_backoff_and_cancellable: Option<BackoffAndCancellable>,
    ) -> Self {
        Self {
            executor,
            picker,
            saver,
            num_workers,
            picker_backoff_and_cancellable,
        }
    }

    /// Runs job runner tasks.
    pub fn run(self) -> Vec<JoinHandle<anyhow::Result<()>>> {
        let (input_tx, input_rx) =
            tokio::sync::mpsc::channel::<(E::Input, E::Metadata)>(CHANNEL_SIZE);
        let (result_tx, result_rx) =
            tokio::sync::mpsc::channel::<(anyhow::Result<E::Output>, E::Metadata)>(CHANNEL_SIZE);

        let picker_task =
            JobPickerTask::new(self.picker, input_tx, self.picker_backoff_and_cancellable);
        let worker_pool = WorkerPool::new(self.executor, self.num_workers, input_rx, result_tx);
        let saver_task = JobSaverTask::new(self.saver, result_rx);

        vec![
            tokio::spawn(picker_task.run()),
            tokio::spawn(worker_pool.run()),
            tokio::spawn(saver_task.run()),
        ]
    }
}
