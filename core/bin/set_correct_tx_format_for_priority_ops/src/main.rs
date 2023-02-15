use std::thread::sleep;
use std::time::Duration;
use zksync_dal::ConnectionPool;

#[tokio::main]
async fn main() {
    let pool = ConnectionPool::new(Some(1), true);
    let mut storage = pool.access_storage().await;

    while storage
        .transactions_dal()
        .set_correct_tx_type_for_priority_operations(40)
    {
        println!("Some txs were updated");
        sleep(Duration::from_secs(1));
    }
    println!("finish");
}
