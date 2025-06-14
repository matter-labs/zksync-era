// src/erc20_worker.rs
//! ERC-20 `transfer()` load worker with client-side nonce tracking.

use crate::{erc20::SimpleERC20, metrics::Metrics};
use ethers::{
    prelude::*,
    types::U256,
};
use parking_lot::RwLock;
use rand::{rngs::StdRng, seq::SliceRandom};
use rand_distr::{Distribution, Normal};
use std::{
    sync::{atomic::{AtomicBool, Ordering}, Arc},
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const JITTER_SIGMA: f64 = 0.05;

pub fn spawn_erc20_workers(
    provider: Provider<Http>,
    wallets: Vec<LocalWallet>,
    gas: U256,
    metrics: Metrics,
    running: Arc<AtomicBool>,
    max_in_flight: u32,
    mean_amt: U256,
    token_addr: Address,
    rng: Arc<RwLock<StdRng>>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let addrs: Vec<_> = wallets.iter().map(|w| w.address()).collect();
    let sems = (0..wallets.len())
        .map(|_| Arc::new(Semaphore::new(max_in_flight as usize)))
        .collect::<Vec<_>>();
    let normal = Normal::new(0.0, JITTER_SIGMA).unwrap();

    wallets
        .into_iter()
        .enumerate()
        .map(|(idx, wallet)| {
            let sem         = sems[idx].clone();
            let provider_c  = provider.clone();
            let addrs_c     = addrs.clone();
            let m           = metrics.clone();
            let running_c   = running.clone();
            let rng_c       = rng.clone();
            let normal_c    = normal;
            let gas_c       = gas;
            let token_addr_c= token_addr;

            tokio::spawn(async move {
                let signer = SignerMiddleware::new(provider_c.clone(), wallet);
                let token  = SimpleERC20::new(token_addr_c, Arc::new(signer.clone()));

                let mut nonce = signer
                    .get_transaction_count(
                        signer.address(),
                        Some(BlockId::Number(BlockNumber::Pending)),
                    )
                    .await
                    .expect("nonce fetch");
                println!("erc20 wallet {idx} start-nonce {nonce}");

                while running_c.load(Ordering::Relaxed) {
                    let permit: OwnedSemaphorePermit = match sem.clone().try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => { tokio::time::sleep(Duration::from_millis(10)).await; continue; }
                    };

                    let dest = loop {
                        let cand = { let mut g = rng_c.write(); *addrs_c.choose(&mut *g).unwrap() };
                        if cand != signer.address() { break cand; }
                    };

                    let delta = { let mut g = rng_c.write(); normal_c.sample(&mut *g) };
                    let mut amt = mean_amt;
                    if delta != 0.0 {
                        let d = U256::from(((mean_amt.as_u128() as f64 * delta.abs()) as u128).min(u128::MAX));
                        amt = if delta.is_sign_positive() { amt.saturating_add(d) } else { amt.saturating_sub(d) };
                    }

                    let t_sub = Instant::now();
                    match token
                        .transfer(H160::random(), amt)
                        .gas(gas_c)
                        .nonce(nonce)
                        .send()
                        .await
                    {
                        Ok(pending) => {
                            nonce += U256::one(); // advance on success

                            let sub_ms = t_sub.elapsed().as_millis() as u64;
                            m.submit.write().record(sub_ms).ok();
                            m.sub_last.lock().push_back((Instant::now(), sub_ms));
                            m.sent.fetch_add(1, Ordering::Relaxed);

                            let tx_hash = *pending;
                            let prov    = provider_c.clone();
                            let m_inc   = m.clone();
                            tokio::spawn(async move {
                                let t_inc = Instant::now();
                                loop {
                                    match prov.get_transaction_receipt(tx_hash).await {
                                        Ok(Some(_)) => {
                                            let inc_ms = t_inc.elapsed().as_millis() as u64;
                                            m_inc.include.write().record(inc_ms).ok();
                                            m_inc.inc_last.lock().push_back((Instant::now(), inc_ms));
                                            m_inc.included.fetch_add(1, Ordering::Relaxed);
                                            break;
                                        }
                                        Ok(None) => tokio::time::sleep(Duration::from_millis(100)).await,
                                        Err(_)   => break,
                                    }
                                }
                                drop(permit);
                            });
                        }
                        Err(e) => {
                            // keep same nonce for retry
                            drop(permit);
                            eprintln!("❗ send error: {e}");
                        }
                    }
                }
            })
        })
        .collect()
}
