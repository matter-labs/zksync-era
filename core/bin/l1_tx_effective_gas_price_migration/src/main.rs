use structopt::StructOpt;
use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "DB migration for setting correct effective_gas_price",
    about = "DB migration for setting correct effective_gas_price"
)]
struct Opt {
    #[structopt(short = "f", long = "first_post_m6_block")]
    first_post_m6_block: u32,
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();
    let first_post_m6_block = opt.first_post_m6_block;
    println!("first_post_m6_block: {first_post_m6_block}");

    let pool = ConnectionPool::new(Some(1), DbVariant::Master).await;
    let mut storage = pool.access_storage().await;

    const BLOCK_RANGE: u32 = 1000;
    println!("Setting effective gas price for pre-M6 transactions");

    let mut from_block_number = 0;
    loop {
        if from_block_number >= first_post_m6_block {
            break;
        }

        let to_block_number =
            std::cmp::min(first_post_m6_block - 1, from_block_number + BLOCK_RANGE - 1);
        println!("Block range {from_block_number}-{to_block_number}");
        storage
            .transactions_dal()
            .migrate_l1_txs_effective_gas_price_pre_m6(from_block_number, to_block_number)
            .await;

        from_block_number = to_block_number + 1;
    }

    println!("Setting effective gas price for post-M6 transactions");

    let current_block_number = storage.blocks_dal().get_sealed_miniblock_number().await;
    let mut from_block_number = first_post_m6_block;
    loop {
        if from_block_number > current_block_number.0 {
            break;
        }

        let to_block_number =
            std::cmp::min(current_block_number.0, from_block_number + BLOCK_RANGE - 1);
        println!("Block range {from_block_number}-{to_block_number}");
        storage
            .transactions_dal()
            .migrate_l1_txs_effective_gas_price_post_m6(from_block_number, to_block_number)
            .await;

        from_block_number = to_block_number + 1;
    }
}
