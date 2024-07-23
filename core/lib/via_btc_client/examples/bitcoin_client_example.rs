#![cfg(feature = "test_utils")]

use anyhow::Result;
use via_btc_client::{client::BitcoinClient, regtest::TestContext, BitcoinOps};

#[tokio::main]
async fn main() -> Result<()> {
    let context = TestContext::setup(None).await;

    let client = BitcoinClient::new(&context.get_url(), "regtest").await?;

    let block_height = client.fetch_block_height().await?;
    println!("Current block height: {}", block_height);

    let estimated_fee = client.estimate_fee(6).await?;
    println!(
        "Estimated fee for 6 confirmations: {} satoshis/vbyte",
        estimated_fee
    );

    Ok(())
}
