use anyhow::Context as _;
use clap::Args as ClapArgs;
use sqlx::{postgres::PgPoolOptions, Row};
use zksync_config::PostgresConfig;
use zksync_env_config::FromEnv;

#[derive(ClapArgs)]
pub(crate) struct Args {
    #[clap(short, long)]
    l1_batch_number: i32,
}

pub(crate) async fn run(args: Args) -> anyhow::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect("postgres://postgres:notsecurepassword@localhost/prover_local")
        .await?;

    let query = sqlx::query(
        "SELECT 
            id, 
            status, 
            error, 
            processing_started_at, 
            time_taken, 
            is_node_final_proof 
        FROM 
            prover_jobs_fri 
        WHERE 
            l1_batch_number = $1",
    )
    .bind(&args.l1_batch_number)
    .fetch_all(&pool)
    .await?;

    let total_jobs = query.len();
    let successful_jobs = query
        .iter()
        .filter(|row| row.get::<String, _>("status") == "successful")
        .count();
    let failed_jobs = query
        .iter()
        .filter(|row| row.get::<String, _>("status") == "failed")
        .count();
    let progress = (successful_jobs as f32 / total_jobs as f32) * 100.0;

    println!("= Prover progress =");
    println!("Batch number: {}", args.l1_batch_number);
    println!(
        "Progress: {:.2}% ({}/{})",
        progress, successful_jobs, total_jobs
    );
    println!("Failed: {}", failed_jobs);

    Ok(())
}
