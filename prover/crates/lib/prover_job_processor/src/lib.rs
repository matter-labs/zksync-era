//! Prover job processor.

pub use executor::Executor;
pub use job_picker::JobPicker;
pub use job_runner::JobRunner;
pub use job_saver::JobSaver;

mod executor;
mod job_picker;
mod job_runner;
mod job_saver;
mod task_wiring;
// pub fn add(left: u64, right: u64) -> u64 {
//     left + right
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn it_works() {
//         let result = add(2, 2);
//         assert_eq!(result, 4);
//     }
// }
