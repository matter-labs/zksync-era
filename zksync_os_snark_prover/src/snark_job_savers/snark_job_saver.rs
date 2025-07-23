// use async_trait::async_trait;
// use zkos_wrapper::SnarkWrapperProof;
// use zksync_prover_job_processor::JobSaver;
// 
// use crate::{
//     client::SequencerProofClient,
//     single_fri_snark_executor::{SingleFriSnarkExecutor, SingleFriSnarkExecutorMetadata},
// };
// 
// pub struct SnarkJobSaver {
//     sequencer_client: SequencerProofClient,
// }
// 
// impl SnarkJobSaver {
//     pub fn new(sequencer_url: String) -> Self {
//         Self {
//             sequencer_client: SequencerProofClient::new(sequencer_url),
//         }
//     }
// }
// 
// #[async_trait]
// impl JobSaver for SnarkJobSaver {
//     type ExecutorType = SingleFriSnarkExecutor;
// 
//     async fn save_job_result(
//         &self,
//         data: (anyhow::Result<SnarkWrapperProof>, SingleFriSnarkExecutorMetadata),
//     ) -> anyhow::Result<()> {
//         let (result, metadata) = data;
//         match result {
//             Ok(proof) => {
//                 self.sequencer_client
//                     .submit_snark_proof(metadata.l2_block_number, proof)
//                     .await
//             }
//             Err(err) => {
//                 println!("Error: Failed to save SNARK job result: {:?}", err);
//                 Err(err)
//             }
//         }
//     }
// }
