//! Reports how much value (per token) is currently escrowed on L1 by the
//! L1 Shared Bridge / Asset Router.
//!
//! The canonical token list comes from the node's Postgres `tokens` table; the
//! live balances are read from an L1 (Ethereum) JSON-RPC endpoint via `balanceOf`
//! (and `eth_getBalance` for ETH). USD prices are fetched live from CoinGecko by
//! L1 contract address (the DB `usd_price` column is unpopulated in practice), and
//! used to value and rank the holdings.
//!
//! Usage:
//!   DATABASE_URL=postgres://... \
//!   L1_RPC_URL=https://eth-mainnet... \
//!   SHARED_BRIDGE_ADDR=0x... \
//!   [COINGECKO_API_KEY=...] \
//!   cargo run --release --bin l1_token_balances

use std::collections::HashMap;

use anyhow::Context as _;
use clap::Parser;
use sqlx::{postgres::PgPoolOptions, Row};
use zksync_types::{Address, U256};

/// CoinGecko's free *no-key* tier allows only ONE contract address per
/// `token_price` request (error 10012). A demo/pro API key raises this, so we
/// batch when a key is present and fall back to one-at-a-time otherwise.
const PRICE_BATCH_WITH_KEY: usize = 100;
const PRICE_BATCH_NO_KEY: usize = 1;

#[derive(Debug, Parser)]
#[command(about = "Report L1 funds escrowed by the shared bridge, per token")]
struct Args {
    /// Postgres connection string for the (external) node DB holding the `tokens` table.
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    /// L1 (Ethereum) JSON-RPC endpoint.
    #[arg(long, env = "L1_RPC_URL")]
    rpc_url: String,
    /// Address of the L1 Shared Bridge / Asset Router that escrows the funds.
    #[arg(long, env = "SHARED_BRIDGE_ADDR")]
    bridge: String,
    /// Only consider tokens flagged `well_known` in the DB.
    #[arg(long)]
    well_known_only: bool,
    /// CoinGecko API base URL (use the pro base if you have a pro key).
    #[arg(
        long,
        env = "COINGECKO_BASE_URL",
        default_value = "https://api.coingecko.com/api/v3"
    )]
    coingecko_base_url: String,
    /// Optional CoinGecko API key (demo or pro, inferred from the base URL).
    #[arg(long, env = "COINGECKO_API_KEY")]
    coingecko_api_key: Option<String>,
}

struct TokenRow {
    l1_address: Address,
    symbol: String,
    decimals: u32,
}

struct ReportRow {
    symbol: String,
    address: Address,
    human: String,
    usd: Option<f64>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let bridge = parse_address(&args.bridge).context("invalid --bridge address")?;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&args.database_url)
        .await
        .context("failed to connect to Postgres")?;

    let mut query = "SELECT l1_address, symbol, decimals FROM tokens".to_string();
    if args.well_known_only {
        query.push_str(" WHERE well_known = TRUE");
    }
    let tokens: Vec<TokenRow> = sqlx::query(&query)
        .fetch_all(&pool)
        .await
        .context("failed to read tokens table")?
        .into_iter()
        .map(|row| {
            let l1: Vec<u8> = row.try_get("l1_address")?;
            anyhow::Ok(TokenRow {
                l1_address: Address::from_slice(&l1),
                symbol: row.try_get("symbol")?,
                decimals: row.try_get::<i32, _>("decimals")? as u32,
            })
        })
        .collect::<anyhow::Result<_>>()?;

    eprintln!(
        "Querying L1 balances of {} tokens held by bridge {:?}...",
        tokens.len(),
        bridge
    );

    // CoinGecko rejects requests without a descriptive User-Agent (HTTP 403).
    let client = reqwest::Client::builder()
        .user_agent("zksync-era-treasury-analytics/1.0")
        .build()
        .context("failed to build HTTP client")?;

    // Step 1: read on-chain balances; keep only the non-zero ones.
    let mut balances: Vec<(TokenRow, U256)> = Vec::new();
    for token in tokens {
        let balance = if token.l1_address.is_zero() {
            eth_get_balance(&client, &args.rpc_url, bridge).await
        } else {
            erc20_balance_of(&client, &args.rpc_url, token.l1_address, bridge).await
        };
        match balance {
            Ok(b) if !b.is_zero() => balances.push((token, b)),
            Ok(_) => {}
            Err(err) => eprintln!(
                "  skipping {} ({:?}): RPC error: {err:#}",
                token.symbol, token.l1_address
            ),
        }
    }

    // Step 2: fetch USD prices for the tokens that actually hold a balance.
    let erc20_addrs: Vec<Address> = balances
        .iter()
        .map(|(t, _)| t.l1_address)
        .filter(|a| !a.is_zero())
        .collect();
    eprintln!(
        "Fetching USD prices for {} tokens from CoinGecko...",
        erc20_addrs.len()
    );
    let prices = fetch_token_prices(&client, &args, &erc20_addrs).await;
    let eth_price = fetch_eth_price(&client, &args).await;

    // Step 3: build the valued report.
    let mut report = Vec::new();
    let mut total_usd = 0.0_f64;
    for (token, balance) in &balances {
        let human = format_units(*balance, token.decimals);
        let price = if token.l1_address.is_zero() {
            eth_price
        } else {
            prices.get(&addr_key(token.l1_address)).copied()
        };
        let usd = price.map(|p| human.parse::<f64>().unwrap_or(0.0) * p);
        if let Some(usd) = usd {
            total_usd += usd;
        }
        report.push(ReportRow {
            symbol: token.symbol.clone(),
            address: token.l1_address,
            human,
            usd,
        });
    }

    // Highest USD value first; tokens without a known price sink to the bottom.
    report.sort_by(|a, b| {
        b.usd
            .unwrap_or(-1.0)
            .partial_cmp(&a.usd.unwrap_or(-1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!(
        "\n{:<12} {:<44} {:>28} {:>18}",
        "SYMBOL", "L1_ADDRESS", "BALANCE", "USD_VALUE"
    );
    println!("{}", "-".repeat(104));
    for row in &report {
        let usd_str = row
            .usd
            .map(|v| format!("{:>18.2}", v))
            .unwrap_or_else(|| format!("{:>18}", "n/a"));
        println!(
            "{:<12} {:<44} {:>28} {usd_str}",
            truncate(&row.symbol, 12),
            format!("{:?}", row.address),
            row.human
        );
    }
    println!("{}", "-".repeat(104));
    let priced = report.iter().filter(|r| r.usd.is_some()).count();
    println!(
        "{} tokens with non-zero balance ({priced} priced) | known USD total: ${:.2}",
        report.len(),
        total_usd
    );

    Ok(())
}

fn parse_address(s: &str) -> anyhow::Result<Address> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).context("not valid hex")?;
    anyhow::ensure!(bytes.len() == 20, "address must be 20 bytes");
    Ok(Address::from_slice(&bytes))
}

/// Lowercase `0x`-prefixed hex, matching how CoinGecko keys its response objects.
fn addr_key(addr: Address) -> String {
    format!("{addr:?}")
}

// --- CoinGecko pricing ---------------------------------------------------------

/// Adds the appropriate CoinGecko auth header for the configured base URL.
fn with_api_key(req: reqwest::RequestBuilder, args: &Args) -> reqwest::RequestBuilder {
    match &args.coingecko_api_key {
        Some(key) if args.coingecko_base_url.contains("pro-api") => {
            req.header("x-cg-pro-api-key", key)
        }
        Some(key) => req.header("x-cg-demo-api-key", key),
        None => req,
    }
}

/// Returns a map of lowercase L1 address -> USD price. Missing tokens are simply absent.
async fn fetch_token_prices(
    client: &reqwest::Client,
    args: &Args,
    addrs: &[Address],
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    let (batch, sleep_ms) = if args.coingecko_api_key.is_some() {
        (PRICE_BATCH_WITH_KEY, 250)
    } else {
        // No key: 1 address per request, and the public tier rate-limits hard.
        if addrs.len() > 1 {
            eprintln!(
                "  CoinGecko: no API key set -> querying {} tokens one-at-a-time (slow, rate-limited). \
                 Set COINGECKO_API_KEY (free demo key) to batch.",
                addrs.len()
            );
        }
        (PRICE_BATCH_NO_KEY, 2500)
    };
    for chunk in addrs.chunks(batch) {
        let list = chunk
            .iter()
            .map(|a| addr_key(*a))
            .collect::<Vec<_>>()
            .join(",");
        let url = format!("{}/simple/token_price/ethereum", args.coingecko_base_url);
        let req = with_api_key(client.get(&url), args).query(&[
            ("contract_addresses", list.as_str()),
            ("vs_currencies", "usd"),
        ]);
        let resp = match req.send().await {
            Ok(r) => r,
            Err(err) => {
                eprintln!("  CoinGecko: price request failed: {err:#}");
                continue;
            }
        };
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            // CoinGecko reports rate-limit / auth problems via the HTTP status + a
            // JSON error envelope. Surface it so "n/a everywhere" is explainable.
            eprintln!("  CoinGecko: HTTP {status}: {}", truncate(body.trim(), 300));
            continue;
        }
        match serde_json::from_str::<HashMap<String, PriceEntry>>(&body) {
            Ok(map) => {
                let before = out.len();
                for (addr, entry) in map {
                    if let Some(usd) = entry.usd {
                        out.insert(addr.to_lowercase(), usd);
                    }
                }
                if out.len() == before {
                    eprintln!(
                        "  CoinGecko: 0 of {} addresses in this batch are listed on the `ethereum` platform.",
                        chunk.len()
                    );
                }
            }
            // A 200 with an error envelope (e.g. {"status":{"error_code":...}}) lands here.
            Err(err) => eprintln!(
                "  CoinGecko: unexpected response ({err}): {}",
                truncate(body.trim(), 300)
            ),
        }
        tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await; // avoid hitting rate limits
    }
    out
}

async fn fetch_eth_price(client: &reqwest::Client, args: &Args) -> Option<f64> {
    let url = format!("{}/simple/price", args.coingecko_base_url);
    let req = with_api_key(client.get(&url), args)
        .query(&[("ids", "ethereum"), ("vs_currencies", "usd")]);
    let resp = match req.send().await {
        Ok(r) => r,
        Err(err) => {
            eprintln!("  CoinGecko: ETH price request failed: {err:#}");
            return None;
        }
    };
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        eprintln!(
            "  CoinGecko: ETH price HTTP {status}: {}",
            truncate(body.trim(), 300)
        );
        return None;
    }
    serde_json::from_str::<HashMap<String, PriceEntry>>(&body)
        .ok()
        .and_then(|m| m.get("ethereum").and_then(|e| e.usd))
}

#[derive(serde::Deserialize)]
struct PriceEntry {
    usd: Option<f64>,
}

// --- L1 RPC --------------------------------------------------------------------

/// `balanceOf(address)` selector + 32-byte padded holder address.
async fn erc20_balance_of(
    client: &reqwest::Client,
    url: &str,
    token: Address,
    holder: Address,
) -> anyhow::Result<U256> {
    let mut data = hex::decode("70a08231").unwrap();
    data.extend_from_slice(&[0u8; 12]);
    data.extend_from_slice(holder.as_bytes());
    eth_call(client, url, token, &data).await
}

async fn eth_call(
    client: &reqwest::Client,
    url: &str,
    to: Address,
    data: &[u8],
) -> anyhow::Result<U256> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [{
            "to": format!("{to:?}"),
            "data": format!("0x{}", hex::encode(data)),
        }, "latest"],
    });
    rpc_u256(client, url, body).await
}

async fn eth_get_balance(
    client: &reqwest::Client,
    url: &str,
    addr: Address,
) -> anyhow::Result<U256> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getBalance",
        "params": [format!("{addr:?}"), "latest"],
    });
    rpc_u256(client, url, body).await
}

async fn rpc_u256(
    client: &reqwest::Client,
    url: &str,
    body: serde_json::Value,
) -> anyhow::Result<U256> {
    let resp: serde_json::Value = client.post(url).json(&body).send().await?.json().await?;
    if let Some(err) = resp.get("error") {
        anyhow::bail!("rpc error: {err}");
    }
    let result = resp
        .get("result")
        .and_then(|v| v.as_str())
        .context("missing result")?;
    let hex_str = result.strip_prefix("0x").unwrap_or(result);
    let padded = if hex_str.len() % 2 == 1 {
        format!("0{hex_str}")
    } else {
        hex_str.to_string()
    };
    let mut bytes = hex::decode(&padded).context("invalid hex in rpc result")?;
    if bytes.len() > 32 {
        bytes = bytes[bytes.len() - 32..].to_vec();
    }
    Ok(U256::from_big_endian(&bytes))
}

// --- formatting ----------------------------------------------------------------

/// Renders a raw integer amount as a human-readable decimal, trimming trailing zeros.
fn format_units(value: U256, decimals: u32) -> String {
    let s = value.to_string();
    let d = decimals as usize;
    if d == 0 {
        return s;
    }
    let (int_part, frac_part) = if s.len() <= d {
        ("0".to_string(), format!("{:0>width$}", s, width = d))
    } else {
        let (i, f) = s.split_at(s.len() - d);
        (i.to_string(), f.to_string())
    };
    let frac_trimmed = frac_part.trim_end_matches('0');
    if frac_trimmed.is_empty() {
        int_part
    } else {
        format!("{int_part}.{frac_trimmed}")
    }
}

/// Truncates by character count (not bytes), so multibyte symbols don't panic.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}
