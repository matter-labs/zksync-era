use clap::Args as ClapArgs;
use zksync_types::L1BatchNumber;

#[derive(ClapArgs)]
pub struct Args {
    /// Batch number to restart
    #[clap(
        short,
        long,
        required_unless_present = "prover_job",
        conflicts_with = "prover_job"
    )]
    batch: Option<L1BatchNumber>,
    /// Prover job to restart
    #[clap(short, long, required_unless_present = "batch")]
    prover_job: Option<u32>,
}

pub async fn run(_args: Args) -> anyhow::Result<()> {
    anyhow::bail!("Not implemented");
}
