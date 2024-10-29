use tokio::task::JoinHandle;

use crate::{BackoffAndCancellable, Executor, JobPicker, JobSaver, task_wiring::{JobPickerTask, JobSaverTask, Task, WorkerPool}};

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
    P: JobPicker<ExecutorType=E>,
    S: JobSaver<ExecutorType=E>,
{
    pub fn new(executor: E, picker: P, saver: S, num_workers: usize, picker_backoff_and_cancellable: Option<BackoffAndCancellable>) -> Self {
        Self {
            executor,
            picker,
            saver,
            num_workers,
            picker_backoff_and_cancellable,
        }
    }

    pub fn run(self) -> Vec<JoinHandle<anyhow::Result<()>>> {
        // TODO: make these 1s a constant all across the codebase
        let (input_tx, input_rx) = tokio::sync::mpsc::channel::<(E::Input, E::Metadata)>(1);
        let (result_tx, result_rx) =
            tokio::sync::mpsc::channel::<(anyhow::Result<E::Output>, E::Metadata)>(1);

        // Create tasks
        let picker_task = JobPickerTask::new(self.picker, input_tx, self.picker_backoff_and_cancellable);
        let worker_pool =
            WorkerPool::new(self.executor, self.num_workers, input_rx, result_tx);
        let saver_task = JobSaverTask::new(self.saver, result_rx);

        vec![
            tokio::spawn(picker_task.run()),
            tokio::spawn(worker_pool.run()),
            tokio::spawn(saver_task.run()),
        ]
    }
}
