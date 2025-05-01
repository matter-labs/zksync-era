use anyhow::Context;
use chrono::{self, NaiveTime};
use clap::{Args, ValueEnum};
use zksync_basic_types::prover_dal::ProofGenerationTime;
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_prover_dal::{Prover, ProverDal};

use crate::cli::ProverCLIConfig;

#[derive(ValueEnum, Clone)]
enum StatsPeriod {
    Day,
    Week,
}

#[derive(Args)]
pub struct Options {
    #[clap(
        short = 'p',
        long = "period",
        help = "Specify the time frame to look for stats",
        default_value = "day"
    )]
    period: StatsPeriod,
}

pub async fn run(opts: Options, config: ProverCLIConfig) -> anyhow::Result<()> {
    let prover_connection_pool = ConnectionPool::<Prover>::singleton(config.db_url)
        .build()
        .await
        .context("failed to build a prover_connection_pool")?;
    let mut conn = prover_connection_pool
        .connection()
        .await
        .context("failed to get connection from pool")?;

    let start_date = match opts.period {
        StatsPeriod::Day => chrono::offset::Local::now().date_naive(),
        StatsPeriod::Week => {
            (chrono::offset::Local::now() - chrono::Duration::days(7)).date_naive()
        }
    };
    let start_date =
        start_date.and_time(NaiveTime::from_num_seconds_from_midnight_opt(0, 0).unwrap());
    let proof_generation_times = conn
        .fri_witness_generator_dal()
        .get_proof_generation_times_for_time_frame(start_date)
        .await?;
    display_proof_generation_time(proof_generation_times);
    Ok(())
}

fn display_proof_generation_time(proof_generation_times: Vec<ProofGenerationTime>) {
    println!("Batch\tTime Taken\t\tCreated At");
    for proof_generation_time in proof_generation_times {
        println!(
            "{}\t{:?}\t\t{}",
            proof_generation_time.batch_id.batch_number().0,
            proof_generation_time.time_taken,
            proof_generation_time.created_at
        );
    }
}
