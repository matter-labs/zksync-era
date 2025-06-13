use std::path::Path;
use std::time::Instant;
use zkos_wrapper::{prove, serialize_to_file, SnarkWrapperProof};
use zksync_airbender_cli::prover_utils::create_final_proofs_from_program_proof;
use zksync_airbender_execution_utils::ProgramProof;
use zksync_prover_job_processor::Executor;
use zksync_types::L2BlockNumber;

fn deserialize_from_file<T: serde::de::DeserializeOwned>(filename: &str) -> T {
    let src = std::fs::File::open(filename).unwrap();
    serde_json::from_reader(src).unwrap()
}

pub struct SnarkExecutor;

impl Executor for SnarkExecutor {
    type Input = ProgramProof;
    type Output = SnarkWrapperProof;
    type Metadata = L2BlockNumber;

    fn execute(
        &self,
        input: Self::Input,
        metadata: Self::Metadata,
    ) -> anyhow::Result<Self::Output> {
        // println!("EMIL -- wth is going on in here?");
        let proof_time = Instant::now();
        let final_proof = create_final_proofs_from_program_proof(input);
        serialize_to_file(&final_proof, Path::new("/home/evl/box/one_fri"));
        // tracing::info!("Three FRIs to one FRI took {:?}", proof_time.elapsed());
        println!("Three FRIs to one FRI took {:?}", proof_time.elapsed());

        let snark_time = Instant::now();
        match prove(
            String::from("/home/evl/box/one_fri"),
            Some(String::from("/home/evl/code/zksync-era/app.bin")),
            // Some(String::from("/home/evl/app.bin")),
            String::from("/home/evl/box/"),
            Some(String::from("/home/evl/code/zkos-wrapper/crs/setup.key")),
        ) {
            Ok(()) => {
                // tracing::info!(
                //     "Snarkification took {:?}, with total proving time being {:?}, find your data in `/home/evl/box/`",
                //     snark_time.elapsed(),
                //     proof_time.elapsed()
                // );
                println!(
                    "Snarkification took {:?}, with total proving time being {:?}, find your data in `/home/evl/box/`",
                    snark_time.elapsed(),
                    proof_time.elapsed()
                );
            }
            Err(e) => {
                // tracing::error!("failed to snarkify proof: {e:?}");
                println!("failed to snarkify proof: {e:?}");
            }
        }
        let snark = deserialize_from_file("/home/evl/box/snark_proof.json");
        Ok(snark)
    }
}
