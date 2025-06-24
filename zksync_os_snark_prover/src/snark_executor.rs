use std::path::Path;
use std::time::Instant;
use zkos_wrapper::{SnarkWrapperProof, prove, serialize_to_file};
use zksync_airbender_cli::prover_utils::create_final_proofs_from_program_proof;
use zksync_airbender_execution_utils::ProgramProof;
use zksync_prover_job_processor::Executor;
use zksync_types::L2BlockNumber;

fn deserialize_from_file<T: serde::de::DeserializeOwned>(filename: &str) -> T {
    let src = std::fs::File::open(filename).unwrap();
    serde_json::from_reader(src).unwrap()
}

pub struct SnarkExecutor {
    pub binary_path: String,
    pub output_dir: String,
    pub trusted_setup_file: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SnarkExecutorMetadata {
    pub l2_block_number: L2BlockNumber,
}

impl Executor for SnarkExecutor {
    type Input = ProgramProof;
    type Output = SnarkWrapperProof;
    type Metadata = SnarkExecutorMetadata;

    fn execute(
        &self,
        input: Self::Input,
        _metadata: Self::Metadata,
    ) -> anyhow::Result<Self::Output> {
        let proof_time = Instant::now();
        let final_proof = create_final_proofs_from_program_proof(input);
        let one_fri_path = Path::new(&self.output_dir).join("one_fri.tmp");

        serialize_to_file(&final_proof, &one_fri_path);
        println!("Three FRIs to one FRI took {:?}", proof_time.elapsed());

        let snark_time = Instant::now();
        match prove(
            one_fri_path.into_os_string().into_string().unwrap(),
            Some(self.binary_path.clone()),
            self.output_dir.clone(),
            self.trusted_setup_file.clone(),
            false,
        ) {
            Ok(()) => {
                println!(
                    "Snarkification took {:?}, with total proving time being {:?}, find your data in `/home/evl/box/`",
                    snark_time.elapsed(),
                    proof_time.elapsed()
                );
            }
            Err(e) => {
                println!("failed to snarkify proof: {e:?}");
            }
        }
        let snark = deserialize_from_file(
            Path::new(&self.output_dir)
                .join("snark_proof.json")
                .to_str()
                .unwrap(),
        );
        Ok(snark)
    }
}
