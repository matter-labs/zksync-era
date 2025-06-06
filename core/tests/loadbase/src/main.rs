use clap::Parser;
use ethers::{
    prelude::*,
    signers::{coins_bip39::English, LocalWallet, MnemonicBuilder},
    types::{BlockId, BlockNumber, NameOrAddress, TransactionRequest, U256},
};
use hdrhistogram::Histogram;
use parking_lot::RwLock;
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use rand_distr::{Distribution, Normal};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use ethers::types::transaction::eip2718::TypedTransaction;
use tokio::{sync::Semaphore, time::interval};

/// σ = 5 % of mean transfer amount
const JITTER_SIGMA: f64 = 0.05;

/// Command‑line arguments
#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// RPC endpoint (HTTP/S)
    #[arg(long)]
    rpc_url: String,

    /// Rich account private key (hex 0x…)
    #[arg(long)]
    rich_privkey: String,

    /// Mnemonic phrase used to deterministically derive load‑test wallets
    #[arg(long, default_value = "viable reason festival matrix story shoulder describe ozone conduct carry puppy layer hill final safe")]
    mnemonic: String,

    /// Number of wallets to derive
    #[arg(long, default_value_t = 10)]
    wallets: u32,

    /// Test duration, e.g. 60s, 5m, 1h
    #[arg(long)]
    duration: humantime::Duration,

    /// Max in‑flight tx per wallet (transaction “window”)
    #[arg(long, default_value_t = 10)]
    max_in_flight: u32,

    /// Mean transfer amount (ethers parse syntax: 0.01ether, 5gwei, …)
    #[arg(long, default_value = "1")]
    amount: String,

    /// Mean transfer amount (ethers parse syntax: 0.01ether, 5gwei, …)
    #[arg(long, default_value = "1000000000000")]
    amount_fund: String,

    /// If set, run `eth_estimateGas` once and reuse the value; otherwise hard‑code 21 000 gas
    #[arg(long)]
    estimate_gas: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    //------------------------------ parse & basic setup -----------------------------------------//
    let args = Args::parse();
    let provider = Provider::<Http>::try_from(&args.rpc_url)?
        .interval(Duration::from_millis(100));
    let chain_id = provider.get_chainid().await?.as_u64();
    let rich_wallet: LocalWallet = args.rich_privkey.parse::<LocalWallet>()?.with_chain_id(chain_id);

    //------------------------------ derive test wallets -----------------------------------------//
    let wallets = derive_wallets(&args.mnemonic, args.wallets, chain_id)?;
    let addresses: Vec<_> = wallets.iter().map(|w| w.address()).collect();

    //------------------------------ prefund ------------------------------------------------------//
    let rich = SignerMiddleware::new(provider.clone(), rich_wallet.clone());
    prefund_wallets(&rich, &addresses, &args.amount_fund).await?;

    //------------------------------ gas limit ----------------------------------------------------//
    let gas_limit = if args.estimate_gas {
        estimate_gas_once(&provider, &rich_wallet.address(), args.amount.parse()?).await?
    } else {
        U256::from(21_000u64)
    };

    //------------------------------ shared metrics ----------------------------------------------//
    let sent = Arc::new(AtomicU64::new(0));
    let included = Arc::new(AtomicU64::new(0));
    let submit_hist = Arc::new(RwLock::new(Histogram::<u64>::new_with_max(60_000, 3)?));
    let include_hist = Arc::new(RwLock::new(Histogram::<u64>::new_with_max(60_000, 3)?));

    //------------------------------ global control ----------------------------------------------//
    let running = Arc::new(AtomicBool::new(true));

    //------------------------------ per‑wallet state --------------------------------------------//
    let semaphores: Vec<_> = (0..args.wallets)
        .map(|_| Arc::new(Semaphore::new(args.max_in_flight as usize)))
        .collect();

    //------------------------------ random amount generator -------------------------------------//
    let mean_amt: U256 = args.amount.parse()?;
    let normal = Normal::new(0.0, JITTER_SIGMA).unwrap();
    let rng = Arc::new(RwLock::new(StdRng::from_entropy()));

    //------------------------------ spawn workers -----------------------------------------------//
    for (idx, wallet) in wallets.into_iter().enumerate() {
        let provider_cl = provider.clone();
        let sem = Arc::clone(&semaphores[idx]);
        let addrs = addresses.clone();
        let sent_cl = Arc::clone(&sent);
        let incl_cl = Arc::clone(&included);
        let sub_hist_cl = Arc::clone(&submit_hist);
        let inc_hist_cl = Arc::clone(&include_hist);
        let running_cl = Arc::clone(&running);
        let rng_cl = Arc::clone(&rng);
        let gas = gas_limit;

        tokio::spawn(async move {
            let signer = SignerMiddleware::new(provider_cl.clone(), wallet);
            let mut nonce = signer
                .get_transaction_count(
                    signer.address(),
                    Some(BlockId::Number(BlockNumber::Pending)),
                )
                .await
                .expect("nonce fetch");

            while running_cl.load(Ordering::Relaxed) {
                //---------------------- max‑in‑flight handling ----------------------------------//
                let permit = match sem.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    }
                };

                //---------------------- random destination (not self) ---------------------------//
                let dest = loop {
                    let d = {
                        let mut rng = rng_cl.write();
                        *addrs.choose(&mut *rng).unwrap()
                    };
                    if d != signer.address() {
                        break d;
                    }
                };

                //---------------------- jitter amount ------------------------------------------//
                let delta = {
                    let mut rng = rng_cl.write();
                    normal.sample(&mut *rng)
                };
                let mut amt = mean_amt;
                if delta != 0.0 {
                    let abs = ((mean_amt.as_u128() as f64 * delta.abs()) as u128).min(u128::MAX);
                    let delta_u256 = U256::from(abs);
                    amt = if delta.is_sign_positive() {
                        amt.saturating_add(delta_u256)
                    } else {
                        amt.saturating_sub(delta_u256)
                    };
                }

                //---------------------- build legacy tx ----------------------------------------//
                let mut tx = TransactionRequest::new();
                tx.to = Some(NameOrAddress::Address(dest));
                tx.value = Some(amt);
                tx.from = Some(signer.address());
                tx.gas = Some(gas);
                tx.nonce = Some(nonce);
                nonce += U256::one();

                //---------------------- submit ------------------------------------------------//
                let submit_start = Instant::now();
                let pending = match signer.send_transaction(tx, None).await {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("❗ send error: {e}");
                        continue;
                    }
                };
                sub_hist_cl
                    .write()
                    .record(submit_start.elapsed().as_millis() as u64)
                    .ok();
                sent_cl.fetch_add(1, Ordering::Relaxed);

                //---------------------- poll for inclusion ------------------------------------//
                let tx_hash = *pending;
                let prov_poll = provider_cl.clone();
                let inc_hist = Arc::clone(&inc_hist_cl);
                let inc_counter = Arc::clone(&incl_cl);
                tokio::spawn(async move {
                    let inc_start = Instant::now();
                    loop {
                        match prov_poll.get_transaction_receipt(tx_hash).await {
                            Ok(Some(_)) => {
                                inc_hist
                                    .write()
                                    .record(inc_start.elapsed().as_millis() as u64)
                                    .ok();
                                inc_counter.fetch_add(1, Ordering::Relaxed);
                                break;
                            }
                            Ok(None) => {
                                println!("🔄 tx {tx_hash:?} not included yet, retrying…");
                                tokio::time::sleep(Duration::from_millis(100)).await
                            },
                            Err(e) => {
                                eprintln!("❗ receipt err: {e}");
                                break;
                            }
                        }
                    }
                    drop(permit); // free slot
                });
            }
        });
    }

    //------------------------------ metrics reporter --------------------------------------------//
    let sent_r = Arc::clone(&sent);
    let incl_r = Arc::clone(&included);
    let sub_hist_r = Arc::clone(&submit_hist);
    let inc_hist_r = Arc::clone(&include_hist);

    let start = Instant::now();
    let reporter = tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(1));
        let mut last_inc = 0;
        loop {
            tick.tick().await;
            let elapsed = start.elapsed().as_secs();
            let sent_now = sent_r.load(Ordering::Relaxed);
            let inc_now = incl_r.load(Ordering::Relaxed);
            let tps = (inc_now - last_inc) as f64;
            last_inc = inc_now;
            let in_flight = sent_now.saturating_sub(inc_now);
            let sub_p50 = sub_hist_r.read().value_at_quantile(0.5);
            let inc_p50 = inc_hist_r.read().value_at_quantile(0.5);
            println!(
                "⏱ {:>4}s | sent {:>6} | in‑flight {:>4} | included {:>6} | TPS {:>5.1} | submit p50 {:>3} ms | include p50 {:>5.2} s",
                elapsed,
                sent_now,
                in_flight,
                inc_now,
                tps,
                sub_p50,
                inc_p50 as f64 / 1_000.0
            );
        }
    });

    //------------------------------ run & graceful shutdown ------------------------------------//
    tokio::time::sleep(*args.duration).await;
    running.store(false, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(1)).await; // one extra tick
    reporter.abort();

    println!(
        "▶ Finished after {} s — sent {}, included {}, left {}",
        args.duration.as_secs(),
        sent.load(Ordering::Relaxed),
        included.load(Ordering::Relaxed),
        sent.load(Ordering::Relaxed) - included.load(Ordering::Relaxed)
    );

    Ok(())
}

//-------------------------------------- helpers -------------------------------------------------//
fn derive_wallets(
    mnemonic: &str,
    count: u32,
    chain_id: u64,
) -> anyhow::Result<Vec<LocalWallet>> {
    let builder = MnemonicBuilder::<English>::default().phrase(mnemonic);
    let mut wallets = Vec::with_capacity(count as usize);
    for i in 0..count {
        wallets.push(builder.clone().index(i)?.build()?.with_chain_id(chain_id));
    }
    Ok(wallets)
}

async fn prefund_wallets<S: Signer + 'static>(
    rich: &SignerMiddleware<Provider<Http>, S>,
    dests: &[Address],
    amount: &str,
) -> anyhow::Result<()> {
    let amt: U256 = amount.parse()?;
    println!("⚙  Prefunding {} wallets with {} wei each…", dests.len(), amt);
    for d in dests {
        rich
            .send_transaction(
                TransactionRequest::pay(*d, amt)
                    .from(rich.signer().address())
                    .gas(U256::from(21_000u64)),
                None,
            )
            .await?
            .await?;
    }
    Ok(())
}

async fn estimate_gas_once<P: JsonRpcClient + 'static>(
    provider: &Provider<P>,
    from: &Address,
    amount: U256,
) -> anyhow::Result<U256> {
    let tx_req = TransactionRequest::pay(*from, amount).from(*from);
    let typed: TypedTransaction = tx_req.into();
    let gas = provider.estimate_gas(&typed, None).await?;
    println!("⚙  Estimated gas limit: {gas}");
    Ok(gas)
}
