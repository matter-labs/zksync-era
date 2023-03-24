#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(dead_code)]
use structopt::StructOpt;

// use crate::commands::gas_price::test_gas_price;
// use crate::commands::revert_block::test_revert_blocks;
// use crate::commands::upgrade_contract::test_upgrade_contract;

mod commands;
mod eth_provider;
mod external_commands;
mod server_handler;
mod tester;
mod types;
mod utils;

#[derive(Debug, StructOpt)]
enum Command {
    All,
    RevertBlock,
    UpgradeContract,
    GasPrice,
}

#[derive(Debug, StructOpt)]
#[structopt(name = "testkit", author = "Matter Labs")]
struct Opt {
    #[structopt(subcommand)]
    command: Command,

    #[structopt(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    if opt.debug {
        let _sentry_guard = vlog::init();
    }

    match opt.command {
        Command::All => {
            // println!("Start contract upgrade test");
            // test_upgrade_contract().await;
            // println!();

            // println!("Start gas price test");
            // test_gas_price().await;
            // println!();

            // println!("Start revert blocks test");
            // test_revert_blocks().await;
            // println!();
        }
        Command::RevertBlock => {
            // println!("Start revert blocks test");
            // test_revert_blocks().await;
        }
        Command::UpgradeContract => {
            // println!("Start contract upgrade test");
            // test_upgrade_contract().await;
        }
        Command::GasPrice => {
            // println!("Start gas price test");
            // test_gas_price().await;
        }
    }

    Ok(())
}
