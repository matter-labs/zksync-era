// use air_compiler_cli::prover_utils::{create_proofs_internal, create_recursion_proofs, program_proof_from_proof_list_and_metadata, GpuSharedState};
// use air_compiler_cli::Machine;
// use execution_utils::ProgramProof;
// use std::sync::Mutex;
// use std::time::Instant;
// // use std::path::Path;
// // use std::time::Instant;
// // use zkos_wrapper::{prove, serialize_to_file, SnarkWrapperProof};
// // use zksync_airbender_cli::prover_utils::create_final_proofs_from_program_proof;
// // use zksync_airbender_execution_utils::ProgramProof;
// use zksync_prover_job_processor::Executor;
// use zksync_types::L2BlockNumber;
// // use zksync_types::L2BlockNumber;
// //
// // fn deserialize_from_file<T: serde::de::DeserializeOwned>(filename: &str) -> T {
// //     let src = std::fs::File::open(filename).unwrap();
// //     serde_json::from_reader(src).unwrap()
// // }
// 
// pub struct FriExecutor {
//     gpu_state: Mutex<GpuSharedState>,
//     app_binary: Vec<u32>,
// }
// 
// impl FriExecutor {
//     pub fn new(gpu_state: GpuSharedState, app_binary: Vec<u32>) -> Self {
//         Self {
//             gpu_state: Mutex::new(gpu_state),
//             app_binary,
//         }
//     }
// }
// 
// impl Executor for FriExecutor {
//     type Input = Vec<u32>;
//     type Output = ProgramProof;
//     type Metadata = L2BlockNumber;
// 
//     fn execute(&self, input: Self::Input, metadata: Self::Metadata) -> anyhow::Result<Self::Output> {
//         let time = Instant::now();
//         let mut gpu_state = self.gpu_state.lock().map_err(|e| anyhow::anyhow!("Failed to lock GPU state: {}", e))?;
//         // TODO: most of the code is copied from zksmith, figure out what needs to be done here long-term
//         let (proof_list, proof_metadata) = create_proofs_internal(
//             &self.app_binary,
//             input,
//             &Machine::Standard,
//             // FIXME: figure out how many instances (currently gpu ignores this).
//             100,
//             None,
//             &mut Some(&mut gpu_state),
//         );
//         let (recursion_proof_list, recursion_proof_metadata) = create_recursion_proofs(
//             proof_list,
//             proof_metadata,
//             &None,
//             &mut Some(&mut gpu_state),
//         );
//         drop(gpu_state);
//         tracing::info!(
//             "FriExecutor took {:?} to create proofs for block {}",
//             time.elapsed(),
//             metadata
//         );
//         Ok(program_proof_from_proof_list_and_metadata(&recursion_proof_list, &recursion_proof_metadata))
//     }
// }
// 
// 
// // pub struct SnarkExecutor;
// //
// // impl Executor for SnarkExecutor {
// //     type Input = ProgramProof;
// //     type Output = SnarkWrapperProof;
// //     type Metadata = L2BlockNumber;
// //
// //     fn execute(
// //         &self,
// //         input: Self::Input,
// //         metadata: Self::Metadata,
// //     ) -> anyhow::Result<Self::Output> {
// //         // println!("EMIL -- wth is going on in here?");
// //         let proof_time = Instant::now();
// //         let final_proof = create_final_proofs_from_program_proof(input);
// //         serialize_to_file(&final_proof, Path::new("/home/evl/box/one_fri"));
// //         // tracing::info!("Three FRIs to one FRI took {:?}", proof_time.elapsed());
// //         println!("Three FRIs to one FRI took {:?}", proof_time.elapsed());
// //
// //         let snark_time = Instant::now();
// //         match prove(
// //             String::from("/home/evl/box/one_fri"),
// //             Some(String::from("/home/evl/code/zksync-era/app.bin")),
// //             // Some(String::from("/home/evl/app.bin")),
// //             String::from("/home/evl/box/"),
// //             Some(String::from("/home/evl/code/zkos-wrapper/crs/setup.key")),
// //         ) {
// //             Ok(()) => {
// //                 // tracing::info!(
// //                 //     "Snarkification took {:?}, with total proving time being {:?}, find your data in `/home/evl/box/`",
// //                 //     snark_time.elapsed(),
// //                 //     proof_time.elapsed()
// //                 // );
// //                 println!(
// //                     "Snarkification took {:?}, with total proving time being {:?}, find your data in `/home/evl/box/`",
// //                     snark_time.elapsed(),
// //                     proof_time.elapsed()
// //                 );
// //             }
// //             Err(e) => {
// //                 // tracing::error!("failed to snarkify proof: {e:?}");
// //                 println!("failed to snarkify proof: {e:?}");
// //             }
// //         }
// //         let snark = deserialize_from_file("/home/evl/box/snark_proof.json");
// //         Ok(snark)
// //     }
// // }
