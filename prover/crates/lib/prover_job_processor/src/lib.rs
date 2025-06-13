mod backoff_and_cancellable;
mod executor;
mod job_picker;
mod job_runner;
mod job_saver;
mod task_wiring;
mod async_job_runner;

pub use backoff_and_cancellable::{Backoff, BackoffAndCancellable};
pub use executor::Executor;
pub use job_picker::JobPicker;
pub use job_runner::JobRunner;
pub use job_saver::JobSaver;
pub use async_job_runner::AsyncJobRunner;

// convenience aliases to simplify declarations
type Input<P> = <<P as JobPicker>::ExecutorType as Executor>::Input;
type PickerMetadata<P> = <<P as JobPicker>::ExecutorType as Executor>::Metadata;

type Output<S> = <<S as JobSaver>::ExecutorType as Executor>::Output;
type SaverMetadata<S> = <<S as JobSaver>::ExecutorType as Executor>::Metadata;
