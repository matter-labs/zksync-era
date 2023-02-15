use zksync_dal::ConnectionPool;
use zksync_types::MiniblockNumber;

#[tokio::main]
async fn main() {
    let pool = ConnectionPool::new(Some(1), true);
    let mut storage = pool.access_storage().await;
    let last_sealed_miniblock = storage.blocks_dal().get_sealed_miniblock_number();

    let mut current_miniblock_number = MiniblockNumber(0);
    let block_range = 10000u32;
    while current_miniblock_number <= last_sealed_miniblock {
        let to_miniblock_number = current_miniblock_number + block_range - 1;
        storage
            .events_dal()
            .set_tx_initiator_address(current_miniblock_number, to_miniblock_number);
        println!(
            "Processed miniblocks {}-{}",
            current_miniblock_number, to_miniblock_number
        );

        current_miniblock_number += block_range;
    }
}
