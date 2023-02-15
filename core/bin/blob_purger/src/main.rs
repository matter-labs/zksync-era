use std::str::FromStr;
use std::thread::sleep;
use std::time::Duration;

use structopt::StructOpt;

use zksync_dal::ConnectionPool;
const WAIT_TIME_MILLI_SECONDS: u64 = 2500;

#[derive(Debug)]
enum BlobTable {
    WitnessInputs,
    LeafAggregationWitnessJobs,
    NodeAggregationWitnessJobs,
    SchedulerWitnessJobs,
    ProverJobs,
}

impl FromStr for BlobTable {
    type Err = String;
    fn from_str(table_name: &str) -> Result<Self, Self::Err> {
        match table_name {
            "witness_inputs" => Ok(BlobTable::WitnessInputs),
            "leaf_aggregation_witness_jobs" => Ok(BlobTable::LeafAggregationWitnessJobs),
            "node_aggregation_witness_jobs" => Ok(BlobTable::NodeAggregationWitnessJobs),
            "scheduler_witness_jobs" => Ok(BlobTable::SchedulerWitnessJobs),
            "prover_jobs" => Ok(BlobTable::ProverJobs),
            _ => Err("Could not parse table name".to_string()),
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Tool for purging blob from database",
    about = "Tool to delete blob for individual tables from db"
)]
struct Opt {
    /// Name of the table from which blobs would be deleted.
    #[structopt(short = "t", long = "table_name", default_value = "witness_inputs")]
    table: BlobTable,
    /// Number of blobs purged in each batch.
    #[structopt(short = "b", long = "batch_size", default_value = "20")]
    batch_size: u8,
}

fn purge_witness_inputs(pool: ConnectionPool, batch_size: u8) -> bool {
    let l1_batches = pool
        .access_storage_blocking()
        .blocks_dal()
        .get_l1_batches_with_blobs_in_db(batch_size);
    if l1_batches.is_empty() {
        return false;
    }
    println!("purging witness_inputs: {:?}", l1_batches);
    pool.access_storage_blocking()
        .blocks_dal()
        .purge_blobs_from_db(l1_batches);
    true
}

fn purge_leaf_aggregation_witness_jobs(pool: ConnectionPool, batch_size: u8) -> bool {
    let l1_batches = pool
        .access_storage_blocking()
        .witness_generator_dal()
        .get_leaf_aggregation_l1_batches_with_blobs_in_db(batch_size);
    if l1_batches.is_empty() {
        return false;
    }
    println!("purging leaf_aggregation_witness_jobs: {:?}", l1_batches);
    pool.access_storage_blocking()
        .witness_generator_dal()
        .purge_leaf_aggregation_blobs_from_db(l1_batches);
    true
}

fn purge_node_aggregation_witness_jobs(pool: ConnectionPool, batch_size: u8) -> bool {
    let l1_batches = pool
        .access_storage_blocking()
        .witness_generator_dal()
        .get_node_aggregation_l1_batches_with_blobs_in_db(batch_size);
    if l1_batches.is_empty() {
        return false;
    }
    println!("purging node_aggregation_witness_jobs: {:?}", l1_batches);
    pool.access_storage_blocking()
        .witness_generator_dal()
        .purge_node_aggregation_blobs_from_db(l1_batches);
    true
}

fn purge_scheduler_witness_jobs(pool: ConnectionPool, batch_size: u8) -> bool {
    let l1_batches = pool
        .access_storage_blocking()
        .witness_generator_dal()
        .get_scheduler_l1_batches_with_blobs_in_db(batch_size);
    if l1_batches.is_empty() {
        return false;
    }
    println!("purging scheduler_witness_jobs: {:?}", l1_batches);
    pool.access_storage_blocking()
        .witness_generator_dal()
        .purge_scheduler_blobs_from_db(l1_batches);
    true
}

fn purge_prover_jobs(pool: ConnectionPool, batch_size: u8) -> bool {
    let job_ids = pool
        .access_storage_blocking()
        .prover_dal()
        .get_l1_batches_with_blobs_in_db(batch_size);
    if job_ids.is_empty() {
        return false;
    }
    println!("purging prover_jobs: {:?}", job_ids);
    pool.access_storage_blocking()
        .prover_dal()
        .purge_blobs_from_db(job_ids);
    true
}

fn main() {
    let opt = Opt::from_args();
    println!("processing table: {:?}", opt.table);
    let pool = ConnectionPool::new(Some(1), true);
    let mut shall_purge = true;
    while shall_purge {
        shall_purge = match opt.table {
            BlobTable::WitnessInputs => purge_witness_inputs(pool.clone(), opt.batch_size),
            BlobTable::LeafAggregationWitnessJobs => {
                purge_leaf_aggregation_witness_jobs(pool.clone(), opt.batch_size)
            }
            BlobTable::NodeAggregationWitnessJobs => {
                purge_node_aggregation_witness_jobs(pool.clone(), opt.batch_size)
            }
            BlobTable::SchedulerWitnessJobs => {
                purge_scheduler_witness_jobs(pool.clone(), opt.batch_size)
            }
            BlobTable::ProverJobs => purge_prover_jobs(pool.clone(), opt.batch_size),
        };
        sleep(Duration::from_millis(WAIT_TIME_MILLI_SECONDS));
    }
}
