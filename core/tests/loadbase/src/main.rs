// src/main.rs
mod wallets;
mod metrics;
mod gas;
mod worker;
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
use rand::{rngs::StdRng, SeedableRng};
use std::{
    sync::{atomic::{AtomicBool, Ordering}, Arc},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, ValueEnum)]
enum TestMode { Native, Erc20 }

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
    #[arg(long, default_value = "1")]  amount: String,
    #[arg(long, default_value = "100000000000000")] amount_fund: String,
    #[arg(long)] estimate_gas: bool,
    #[arg(long, value_enum, default_value_t = TestMode::Native)] mode: TestMode,

    // ERC-20 specific
    #[arg(long)] erc20_address: Option<String>,
    #[arg(long, default_value = "TestToken")] erc20_name: String,
    #[arg(long, default_value = "TTK")]       erc20_symbol: String,
}

// ─── helper: wait until every wallet shows ≥ min wei ───
async fn wait_for_eth(
    provider: &Provider<Http>,
    addrs: &[Address],
    min: U256,
) -> anyhow::Result<()> {
    println!("⏳ waiting for ETH balances …");
    loop {
        let mut ok  = true;
        let mut low = U256::MAX;
        for a in addrs {
            let bal = provider.get_balance(*a, None).await?;
            if bal < min { ok = false; low = low.min(bal); }
        }
        if ok {
            println!("✅ all wallets funded (≥ {min} wei)\n");
            break;

        } else {
            print!("\r   lowest balance so far: {low}");

            std::io::Write::flush(&mut std::io::stdout()).ok();
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ─── provider / chain ───
    let args = Args::parse();
    let provider = Provider::<Http>::try_from(&args.rpc_url)?
        .interval(Duration::from_millis(100));
    let chain_id = provider.get_chainid().await?.as_u64();

    // ─── rich account signers ───
    let rich_wallet: LocalWallet =
        args.rich_privkey.parse::<LocalWallet>()?.with_chain_id(chain_id);

    // plain provider signer (deploy / misc)
    let rich_std_arc = Arc::new(SignerMiddleware::new(provider.clone(), rich_wallet.clone()));

    // Nonce-managed signer for load traffic
    let nm          = NonceManagerMiddleware::new(provider.clone(), rich_wallet.address());
    let rich_nm_arc = Arc::new(SignerMiddleware::new(nm, rich_wallet.clone()));

    // ─── derived wallets ───
    let wallets = wallets::derive(&args.mnemonic, args.wallets, chain_id)?;
    let addrs: Vec<_> = wallets.iter().map(|w| w.address()).collect();

    // ─── shared state ───
    let mean_amt:    U256 = args.amount.parse()?;
    let prefund_amt: U256 = args.amount_fund.parse()?;
    let metrics  = Metrics::new()?;
    let running  = Arc::new(AtomicBool::new(true));
    let rng      = Arc::new(RwLock::new(StdRng::from_entropy()));

    // ─── prefund wallets with ETH ───
    wallets::prefund(&*rich_std_arc, &addrs, &args.amount_fund).await?;
    wait_for_eth(&provider, &addrs, prefund_amt).await?;

    // ───────────────────────── native ETH benchmark ─────────────────────────
    if matches!(args.mode, TestMode::Native) {
        let gas = resolve(
            &provider,
            rich_wallet.address(),
            mean_amt,
            if args.estimate_gas { GasMode::EstimateOnce }
            else                 { GasMode::Fixed(U256::from(21_000u64)) },
        ).await?;

        metrics.spawn_reporter(provider.clone(), addrs.clone(), Instant::now(), None);
        worker::spawn_workers(
            provider.clone(), wallets.clone(), gas, metrics.clone(),
            running.clone(), args.max_in_flight, mean_amt, rng.clone(),
        );
        println!("▶ Native ETH test started with {} wallets\n", args.wallets);
    }

    // ───────────────────────── ERC-20 benchmark ────────────────────────────
    if matches!(args.mode, TestMode::Erc20) {
        use erc20::SimpleERC20;

        // 1) deploy or attach using the *plain* signer (matches erc20.rs sig)
        let token_std: SimpleERC20<_> = match &args.erc20_address {
                        Some(hex) => {
                            let addr: Address = hex.parse::<Address>()?;
                            SimpleERC20::new(addr, rich_std_arc.clone())
                        },
            None => erc20::deploy_and_mint(
                rich_std_arc.clone(),
                &args.erc20_name,
                &args.erc20_symbol,
                prefund_amt * U256::from(args.wallets),
            )
                .await?,
        };
        println!("ERC-20 at {}\n", token_std.address());

        // 2) re-wrap token interface with the NonceManager signer
        let token_nm = SimpleERC20::new(token_std.address(), rich_nm_arc.clone());

        // 3) distribute tokens
        erc20::distribute(&token_nm, &addrs, prefund_amt).await?;

        // 4) gas for token transfers
        let gas = resolve(
            &provider,
            rich_wallet.address(),
            U256::zero(),
            if args.estimate_gas { GasMode::EstimateOnce }
            else                 { GasMode::Fixed(U256::from(60_000u64)) },
        ).await?;

        metrics.spawn_reporter(
            provider.clone(), addrs.clone(), Instant::now(), Some(token_nm.address())
        );
        erc20_worker::spawn_erc20_workers(
            provider.clone(), wallets.clone(), gas, metrics.clone(),
            running.clone(), args.max_in_flight, mean_amt,
            token_nm.address(), rng.clone(),
        );
        println!("▶ ERC-20 test started with {} wallets\n", args.wallets);
    }

    // ─── run for duration ───
    tokio::time::sleep(*args.duration).await;
    running.store(false, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("▶ Test finished");
    Ok(())
}
