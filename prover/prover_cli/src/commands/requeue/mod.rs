use clap::Subcommand;

pub(crate) mod basic_witness_generator_jobs;

#[derive(Subcommand)]
pub enum RequeueCommand {
    BasicWitnessGeneratorJobs(basic_witness_generator_jobs::Args),
}

impl RequeueCommand {
    pub(crate) async fn run(self) -> anyhow::Result<()> {
        match self {
            RequeueCommand::BasicWitnessGeneratorJobs(args) => {
                basic_witness_generator_jobs::run(args).await
            }
        }
    }
}
