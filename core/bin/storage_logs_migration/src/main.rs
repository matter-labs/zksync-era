use zksync_dal::ConnectionPool;
use zksync_types::L1BatchNumber;

#[tokio::main]
async fn main() {
    let pool = ConnectionPool::new(Some(1), true);
    let mut storage = pool.access_storage().await;
    let last_sealed_l1_batch = storage.blocks_dal().get_sealed_block_number();

    let mut current_l1_batch_number = L1BatchNumber(0);
    let block_range = 100u32;
    while current_l1_batch_number <= last_sealed_l1_batch {
        let to_l1_batch_number = current_l1_batch_number + block_range - 1;
        storage
            .storage_logs_dedup_dal()
            .migrate_protective_reads(current_l1_batch_number, to_l1_batch_number);
        storage
            .storage_logs_dedup_dal()
            .migrate_initial_writes(current_l1_batch_number, to_l1_batch_number);
        println!(
            "Processed l1 batches {}-{}",
            current_l1_batch_number, to_l1_batch_number
        );

        current_l1_batch_number += block_range;
    }
}
