use std::sync::Arc;

use tokio::task::JoinHandle;

use crate::{
    task_wiring::{JobPickerTask, JobSaverTask, Task, WorkerPool},
    Executor, JobPicker, JobSaver,
};

pub struct JobRunner<E, P, S>
where
    E: Executor,
    P: JobPicker,
    S: JobSaver,
{
    executor: Arc<E>,
    picker: Arc<P>,
    saver: Arc<S>,
    num_workers: usize,
}

impl<E, P, S> JobRunner<E, P, S>
where
    E: Executor,
    P: JobPicker<ExecutorType = E>,
    S: JobSaver<ExecutorType = E>,
{
    pub fn new(executor: E, picker: P, saver: S, num_workers: usize) -> Self {
        Self {
            executor: Arc::new(executor),
            picker: Arc::new(picker),
            saver: Arc::new(saver),
            num_workers,
        }
    }

    pub fn run(&self) -> Vec<JoinHandle<anyhow::Result<()>>> {
        // TODO: make these 1s a constant all across the codebase
        let (input_tx, input_rx) = tokio::sync::mpsc::channel::<(E::Input, E::Metadata)>(1);
        let (result_tx, result_rx) =
            tokio::sync::mpsc::channel::<(anyhow::Result<E::Output>, E::Metadata)>(1);

        // Create tasks
        let picker_task = JobPickerTask::new(self.picker.clone(), input_tx);
        // TODO: Get rid of worker_pool_task
        let worker_pool =
            WorkerPool::new(self.executor.clone(), self.num_workers, input_rx, result_tx);
        // let worker_pool_task = WorkerPoolTask::new(
        //     self.executor.clone(),
        //     self.num_workers,
        //     input_rx,
        //     result_tx,
        // );
        let saver_task = JobSaverTask::new(self.saver.clone(), result_rx);

        vec![
            tokio::spawn(picker_task.run()),
            tokio::spawn(worker_pool.run()),
            tokio::spawn(saver_task.run()),
        ]
    }
}
