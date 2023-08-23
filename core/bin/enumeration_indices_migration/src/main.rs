use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;

#[tokio::main]
async fn main() {
    vlog::init();

    let pool = ConnectionPool::singleton(DbVariant::Master).build().await;
    let mut storage = pool.access_storage().await;
    zksync_core::state_keeper::set_missing_initial_writes_indices(&mut storage).await;
}
