#![feature(allocator_api)]
#![feature(generic_const_exprs)]

pub mod zkos_prover_input_generator;
mod zkos_proof_data_server;
mod proof_verifier;

pub use zkos_prover_input_generator::ZkosProverInputGenerator;