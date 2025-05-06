#![allow(incomplete_features)]
// Crypto code uses generic const exprs, allocator_api is needed to use global allocators
#![feature(generic_const_exprs)]

pub mod job_runner;
mod metrics;
pub mod proof_fri_compressor_executor;
pub mod proof_fri_compressor_job_picker;
pub mod proof_fri_compressor_job_saver;
pub mod proof_fri_compressor_metadata;
pub mod proof_fri_compressor_payload;

pub use proof_fri_compressor_executor::ProofFriCompressorExecutor;
pub use proof_fri_compressor_job_picker::ProofFriCompressorJobPicker;
pub use proof_fri_compressor_job_saver::ProofFriCompressorJobSaver;
