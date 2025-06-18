mod wallets;
mod metrics;
mod gas;
mod erc20;
mod erc20_worker;

use clap::{Parser, ValueEnum};
use ethers::{
    middleware::NonceManagerMiddleware,
    prelude::*,
    types::U256,
};
use gas::{resolve, GasMode};
use metrics::Metrics;
use parking_lot::RwLock;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    sync::{atomic::{AtomicBool, Ordering}, Arc},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, ValueEnum)]
enum DestMode { Wallet, Random }

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(long)] rpc_url: String,
    #[arg(long)] rich_privkey: String,
    #[arg(long, default_value = "legal winner thank year wave sausage worth useful legal winner thank yellow")]
    mnemonic: String,
    #[arg(long, default_value_t = 10)] wallets: u32,
    #[arg(long)] duration: humantime::Duration,
    #[arg(long, default_value_t = 10)] max_in_flight: u32,
    #[arg(long, default_value = "100000000000000")] amount_fund: String,
    #[arg(long)] estimate_gas: bool,
    #[arg(long, value_enum, default_value_t = DestMode::Wallet)] dest: DestMode,
    #[arg(long)] erc20_address: Option<String>,
    #[arg(long, default_value = "TestToken")] erc20_name: String,
    #[arg(long, default_value = "TTK")]       erc20_symbol: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    //-------------------------------- env ----------------------------------//
    let args = Args::parse();
    let provider = Provider::<Http>::try_from(&args.rpc_url)?
        .interval(Duration::from_millis(100));
    let chain_id = provider.get_chainid().await?.as_u64();

    //-------------------------------- signers ------------------------------//
    let rich_wallet: LocalWallet =
        args.rich_privkey.parse::<LocalWallet>()?.with_chain_id(chain_id);
    let rich_std_arc = Arc::new(SignerMiddleware::new(provider.clone(), rich_wallet.clone()));
    let rich_nm_arc  = Arc::new(SignerMiddleware::new(
        NonceManagerMiddleware::new(provider.clone(), rich_wallet.address()),
        rich_wallet.clone(),
    ));

    //-------------------------------- wallets + ETH prefund ---------------//
    let wallets = wallets::derive(&args.mnemonic, args.wallets, chain_id)?;
    let addrs: Vec<_> = wallets.iter().map(|w| w.address()).collect();

    let base_eth: U256 = args.amount_fund.parse()?;
    let mut rng = StdRng::from_entropy();
    let eth_amounts: Vec<U256> = (0..addrs.len())
        .map(|_| U256::from(((base_eth.as_u128() as f64) * rng.gen_range(1.5..3.5)) as u128))
        .collect();
    wallets::prefund_varied(&*rich_std_arc, &addrs, &eth_amounts).await?;

    //-------------------------------- transfer size ------------------------//
    let mut list: Vec<u128> = eth_amounts.iter().map(|x| x.as_u128()).collect();
    list.sort_unstable();
    let mean_transfer = U256::from(list[list.len()/2] / 2); // 50 %

    //-------------------------------- ERC-20 deploy/distribute -------------//
    use erc20::{SimpleERC20, distribute_varied};
    let supply = U256::from_dec_str("1000000000000000000000000000000")?; // 1e6 tokens
    let per_wallet = supply / U256::from(args.wallets);
    let token_amounts = vec![per_wallet; args.wallets as usize];

    let token_std: SimpleERC20<_> = match &args.erc20_address {
        Some(hex) => SimpleERC20::new(hex.parse::<Address>()?, rich_std_arc.clone()),
        None => erc20::deploy_and_mint(
            rich_std_arc.clone(),
            &args.erc20_name,
            &args.erc20_symbol,
            supply,
        ).await?,
    };
    println!("ERC-20 at {}\n", token_std.address());

    let token_nm = SimpleERC20::new(token_std.address(), rich_nm_arc.clone());
    distribute_varied(&token_nm, &addrs, &token_amounts).await?;

    //-------------------------------- metrics & workers --------------------//
    let metrics = Metrics::new()?;
    metrics.spawn_reporter(Instant::now());        // start *after* distribution

    let running = Arc::new(AtomicBool::new(true));
    let rng_arc = Arc::new(RwLock::new(StdRng::from_entropy()));
    let dest_rand = matches!(args.dest, DestMode::Random);

    let gas = resolve(
        &provider,
        rich_wallet.address(),
        U256::zero(),
        if args.estimate_gas { GasMode::EstimateOnce }
        else                 { GasMode::Fixed(U256::from(60_000)) },
    ).await?;

    erc20_worker::spawn_erc20_workers(
        provider.clone(), wallets.clone(), gas, metrics.clone(),
        running.clone(), args.max_in_flight, mean_transfer,
        token_nm.address(), rng_arc.clone(), dest_rand,
    );
    println!("▶ ERC-20 test started with {} wallets\n", args.wallets);

    //-------------------------------- run ----------------------------------//
    tokio::time::sleep(*args.duration).await;
    running.store(false, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("▶ Test finished");
    Ok(())
}
