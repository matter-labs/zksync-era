//! Ranks the L2 addresses (EOAs and contracts) that hold the most value:
//! base token (ETH) + ERC20 holdings, priced via CoinGecko.
//!
//! How it stays tractable on a huge chain:
//!   * Candidate addresses come from the `transactions` table (initiator ∪ target),
//!     scanned in bounded block chunks.
//!   * ERC20 balances are read directly: for each scanned token we compute the
//!     `balanceOf` storage key for every candidate address and batch-read it. This
//!     suits a small, fixed token set (no `events` scan needed). Caveat: an address
//!     that *received* a token but never initiated/was-targeted by a transaction is
//!     not in the candidate set, so a pure-recipient-only holder is missed.
//!   * Balances are read from `storage_logs` (current state), not by RPC. The exact
//!     Blake2s256 storage keys are computed via `zksync_types` (same as the node).
//!
//! Caveats:
//!   * ERC20 balances are only correct for zkSync's *standard* ERC20 layout
//!     (`balanceOf` at slot 51). Custom tokens with a different layout are wrong.
//!   * Pricing is CoinGecko-by-L1-address; testnet token addresses won't be listed
//!     (they show as $0 contribution). Base token (ETH) prices via `--base-cg-id`.
//!   * Pruned chains: only retained `storage_logs` are visible.
//!
//! Resumability:
//!   * On a large chain this is a multi-hour/multi-day job. Pass `--cache-dir DIR` to
//!     checkpoint progress to disk *continuously* — every block chunk and storage
//!     batch is flushed and a cursor advanced as it completes, so you can stop the
//!     process at any time (Ctrl-C, OOM, power loss) and a re-run with the same
//!     `--cache-dir` resumes mid-phase, redoing at most the last in-flight chunk.
//!   * The snapshot block is pinned in `DIR/meta.txt` on the first run and reused on
//!     every resume, so base-token and ERC20 balances stay consistent across restarts
//!     even as the chain advances.
//!
//! Usage:
//!   DATABASE_URL=postgres://... [COINGECKO_API_KEY=...] \
//!   cargo run --release --bin rich_contracts -- --top 50 --cache-dir ./run-cache

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use clap::Parser;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use zksync_types::{
    utils::{storage_key_for_eth_balance, storage_key_for_standard_token_balance},
    AccountTreeId, Address, H256, U256,
};

const PRICE_BATCH_WITH_KEY: usize = 100;
const PRICE_BATCH_NO_KEY: usize = 1;

#[derive(Debug, Parser)]
#[command(about = "Rank L2 addresses by total value held (base token + ERC20)")]
struct Args {
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    /// First L2 block to consider when enumerating addresses. Default: 0.
    #[arg(long)]
    from_block: Option<u32>,
    /// Last L2 block. Default: latest sealed block (also the balance snapshot point).
    #[arg(long)]
    to_block: Option<u32>,
    /// How many top holders to print to stdout.
    #[arg(long, default_value_t = 50)]
    top: usize,
    /// Write the full ranking (all holders, not just `--top`) to this CSV file.
    #[arg(long)]
    csv: Option<String>,
    /// Blocks per enumeration query.
    #[arg(long, default_value_t = 100_000)]
    chunk: u32,
    /// Hashed keys per storage_logs batch lookup.
    #[arg(long, default_value_t = 2_000)]
    storage_batch: usize,
    /// Directory used to checkpoint progress to disk so the run is resumable.
    /// Every block chunk and storage batch is flushed as it completes and a cursor
    /// advanced, so the process can be stopped at any moment and a re-run with the
    /// same `--cache-dir` resumes mid-phase. The snapshot block is pinned in
    /// `meta.txt`. Omit to disable all caching.
    #[arg(long)]
    cache_dir: Option<String>,
    /// Safety cap on the number of candidate addresses (warns if exceeded).
    #[arg(long, default_value_t = 8_000_000)]
    max_addresses: usize,
    /// Skip ERC20 holdings; rank by base token only.
    #[arg(long)]
    no_erc20: bool,
    /// Restrict ERC20s to these L1 token addresses (comma-separated). The base token
    /// (zero address) is always included regardless. Empty = all tokens in the DB.
    #[arg(long, value_delimiter = ',')]
    tokens: Vec<String>,
    /// Extra L2-native token contracts not in the `tokens` table (e.g. ZK), each
    /// `L2ADDR[:COINGECKO_ID[:DECIMALS]]`. COINGECKO_ID prices it via /simple/price
    /// (decimals default 18). Comma-separated.
    #[arg(long, value_delimiter = ',')]
    extra_tokens: Vec<String>,
    /// Base token decimals (ETH = 18).
    #[arg(long, default_value_t = 18)]
    base_decimals: u32,
    /// Base token symbol label for output.
    #[arg(long, default_value = "ETH")]
    base_symbol: String,
    /// CoinGecko coin id for the base token (ETH = "ethereum"). Empty to skip base pricing.
    #[arg(long, default_value = "ethereum")]
    base_cg_id: String,
    #[arg(
        long,
        env = "COINGECKO_BASE_URL",
        default_value = "https://api.coingecko.com/api/v3"
    )]
    coingecko_base_url: String,
    #[arg(long, env = "COINGECKO_API_KEY")]
    coingecko_api_key: Option<String>,
}

struct Token {
    l1_address: Address,
    l2_address: Address,
    symbol: String,
    decimals: u32,
}

/// A token to scan, with its price already resolved.
struct ScanToken {
    l2_address: Address,
    symbol: String,
    decimals: u32,
    price: Option<f64>,
}

struct ExtraToken {
    l2_address: Address,
    symbol: String,
    decimals: u32,
    cg_id: Option<String>,
}

/// Parses `L2ADDR[:COINGECKO_ID[:DECIMALS]]`.
fn parse_extra_token(spec: &str) -> anyhow::Result<ExtraToken> {
    let mut parts = spec.split(':');
    let addr_str = parts.next().context("empty --extra-tokens entry")?;
    let l2_address = parse_address(addr_str)?;
    let cg_id = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let decimals = match parts.next() {
        Some(d) => d
            .trim()
            .parse()
            .context("invalid decimals in --extra-tokens")?,
        None => 18,
    };
    let symbol = cg_id
        .clone()
        .unwrap_or_else(|| format!("{}…", &addr_str[..addr_str.len().min(8)]));
    Ok(ExtraToken {
        l2_address,
        symbol,
        decimals,
        cg_id,
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    anyhow::ensure!(
        args.chunk > 0 && args.storage_batch > 0,
        "chunk/batch must be > 0"
    );

    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect(&args.database_url)
        .await
        .context("failed to connect to Postgres")?;

    // Prepare the on-disk cache layout, if requested.
    let cache = args.cache_dir.as_deref().map(Cache::new);
    if let Some(cache) = &cache {
        cache.init().context("failed to create --cache-dir")?;
        eprintln!("Checkpointing to {}", cache.dir.display());
    }

    // Resolve and PIN the block window. The snapshot block must stay fixed across
    // restarts, otherwise cached base/ERC20 balances would mix different states.
    let mut from_block = args.from_block.unwrap_or(0);
    let mut to_block = match args.to_block {
        Some(b) => b,
        None => {
            let row = sqlx::query("SELECT MAX(number) AS max FROM miniblocks")
                .fetch_one(&pool)
                .await?;
            row.try_get::<Option<i64>, _>("max")?
                .context("no miniblocks in DB")? as u32
        }
    };
    if let Some(cache) = &cache {
        match cache.load_meta()? {
            Some(meta) => {
                if (meta.from_block, meta.to_block) != (from_block, to_block) {
                    eprintln!(
                        "Resuming pinned snapshot from {}: blocks {}..={} \
                         (ignoring computed {}..={}).",
                        cache.meta.display(),
                        meta.from_block,
                        meta.to_block,
                        from_block,
                        to_block
                    );
                }
                from_block = meta.from_block;
                to_block = meta.to_block;
            }
            None => cache.save_meta(&Meta {
                from_block,
                to_block,
            })?,
        }
    }

    // --- 1. Candidate address universe (EOAs + contracts that touched the chain) ---
    eprintln!("Enumerating candidate addresses over blocks {from_block}..={to_block}...");
    let chunk_list = chunks(from_block, to_block, args.chunk);
    let mut universe: HashSet<Address> = HashSet::new();
    let mut start_idx = 0;
    if let Some(cache) = &cache {
        if cache.accounts.exists() {
            universe = read_addresses(&cache.accounts)?.into_iter().collect();
        }
        start_idx = read_cursor(&cache.accounts_cursor);
        if start_idx > 0 {
            eprintln!(
                "  Resuming enumeration: {start_idx}/{} chunks done, {} addresses so far.",
                chunk_list.len(),
                universe.len()
            );
        }
    }
    for (i, (start, end)) in chunk_list.iter().enumerate().skip(start_idx) {
        let addrs = enumerate_addresses_chunk(&pool, *start, *end).await?;
        let mut fresh: Vec<Address> = Vec::new();
        for a in addrs {
            if universe.insert(a) {
                fresh.push(a);
            }
        }
        if let Some(cache) = &cache {
            append(&cache.accounts, &addr_lines(&fresh))?;
            write_cursor(&cache.accounts_cursor, i + 1)?;
        }
        eprintln!("  {start}..={end}: {} candidate addresses", universe.len());
        if universe.len() > args.max_addresses {
            eprintln!(
                "  WARNING: exceeded --max-addresses ({}); stopping enumeration early. \
                 Results cover only the most recent blocks (down to ~{start}).",
                args.max_addresses
            );
            // Mark enumeration "complete" so resumes don't reopen the capped tail.
            if let Some(cache) = &cache {
                write_cursor(&cache.accounts_cursor, chunk_list.len())?;
            }
            break;
        }
    }

    // --- 2. Base token balances for the whole universe ---
    // Use a stable, file-order address list so the batch cursor maps to the same
    // addresses across restarts (HashSet iteration order is not stable).
    let all_addrs: Vec<Address> = match &cache {
        Some(c) if c.accounts.exists() => read_addresses(&c.accounts)?,
        _ => universe.iter().copied().collect(),
    };
    eprintln!(
        "Reading base-token balances for {} addresses...",
        all_addrs.len()
    );
    let mut base_balance: HashMap<Address, U256> = HashMap::new();
    let mut base_done = 0;
    if let Some(cache) = &cache {
        if cache.base_balances.exists() {
            for (a, v) in read_balances(&cache.base_balances)? {
                base_balance.insert(a, v);
            }
        }
        base_done = read_cursor(&cache.base_cursor);
        if base_done > 0 {
            eprintln!(
                "  Resuming base balances: {base_done}/{} addresses done.",
                all_addrs.len()
            );
        }
    }
    while base_done < all_addrs.len() {
        let end = (base_done + args.storage_batch).min(all_addrs.len());
        let keyed: Vec<(Address, H256)> = all_addrs[base_done..end]
            .iter()
            .map(|a| (*a, storage_key_for_eth_balance(a).hashed_key()))
            .collect();
        let nonzero: Vec<(Address, U256)> = fetch_storage_values(&pool, &keyed, to_block)
            .await?
            .into_iter()
            .filter(|(_, v)| !v.is_zero())
            .collect();
        for (a, v) in &nonzero {
            base_balance.insert(*a, *v);
        }
        if let Some(cache) = &cache {
            append(&cache.base_balances, &balance_lines(&nonzero))?;
            write_cursor(&cache.base_cursor, end)?;
        }
        base_done = end;
    }
    eprintln!("  {} addresses hold base token.", base_balance.len());

    // --- 3. ERC20 holdings, scoped per token to its transfer participants ---
    let mut erc20_usd: HashMap<Address, f64> = HashMap::new();
    let mut erc20_holders_seen = 0usize;
    let client = http_client()?;

    if !args.no_erc20 {
        // The zero address denotes the base token (handled separately); drop it here.
        let filter: Vec<Address> = args
            .tokens
            .iter()
            .map(|s| parse_address(s))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .filter(|a| !a.is_zero())
            .collect();
        let db_tokens = load_tokens(&pool, &filter).await?;
        if !filter.is_empty() {
            let found: HashSet<Address> = db_tokens.iter().map(|t| t.l1_address).collect();
            for missing in filter.iter().filter(|a| !found.contains(a)) {
                eprintln!("  WARNING: token {missing:?} not found in the `tokens` table; skipped.");
            }
        }

        // Build a unified, pre-priced scan list. DB tokens are priced by L1 address;
        // extra L2-native tokens are priced by their CoinGecko coin id.
        let l1_prices = fetch_token_prices(
            &client,
            &args,
            &db_tokens.iter().map(|t| t.l1_address).collect::<Vec<_>>(),
        )
        .await;
        let mut scan: Vec<ScanToken> = db_tokens
            .iter()
            .map(|t| ScanToken {
                l2_address: t.l2_address,
                symbol: t.symbol.clone(),
                decimals: t.decimals,
                price: l1_prices.get(&addr_key(t.l1_address)).copied(),
            })
            .collect();
        for spec in &args.extra_tokens {
            let et = parse_extra_token(spec)?;
            let price = match &et.cg_id {
                Some(id) => fetch_simple_price(&client, &args, id).await,
                None => None,
            };
            scan.push(ScanToken {
                l2_address: et.l2_address,
                symbol: et.symbol,
                decimals: et.decimals,
                price,
            });
        }
        eprintln!(
            "Scanning {} ERC20 tokens over {} candidate addresses...",
            scan.len(),
            all_addrs.len()
        );

        for (i, token) in scan.iter().enumerate() {
            let contract = AccountTreeId::new(token.l2_address);
            // Probe every candidate address for this token's balanceOf slot, resuming
            // and checkpointing per storage batch when a cache dir is configured.
            let balances: Vec<(Address, U256)> = match &cache {
                Some(cache) => {
                    let csv = cache.token_path(token.l2_address);
                    let done = cache.token_done(token.l2_address);
                    if !done.exists() {
                        let bcursor = cache.token_bcursor(token.l2_address);
                        let mut bdone = read_cursor(&bcursor);
                        if bdone > 0 {
                            eprintln!(
                                "  [{}/{}] {}: resuming at {bdone}/{} addresses.",
                                i + 1,
                                scan.len(),
                                token.symbol,
                                all_addrs.len()
                            );
                        }
                        while bdone < all_addrs.len() {
                            let end = (bdone + args.storage_batch).min(all_addrs.len());
                            let keyed: Vec<(Address, H256)> = all_addrs[bdone..end]
                                .iter()
                                .map(|a| {
                                    (
                                        *a,
                                        storage_key_for_standard_token_balance(contract, a)
                                            .hashed_key(),
                                    )
                                })
                                .collect();
                            let nonzero: Vec<(Address, U256)> =
                                fetch_storage_values(&pool, &keyed, to_block)
                                    .await?
                                    .into_iter()
                                    .filter(|(_, v)| !v.is_zero())
                                    .collect();
                            append(&csv, &balance_lines(&nonzero))?;
                            bdone = end;
                            write_cursor(&bcursor, bdone)?;
                        }
                        // Atomically mark the token complete so resumes skip it.
                        write_atomic(&done, b"")?;
                    }
                    read_balances(&csv).unwrap_or_default()
                }
                None => {
                    let mut balances: Vec<(Address, U256)> = Vec::new();
                    for chunk in all_addrs.chunks(args.storage_batch) {
                        let keyed: Vec<(Address, H256)> = chunk
                            .iter()
                            .map(|a| {
                                (
                                    *a,
                                    storage_key_for_standard_token_balance(contract, a)
                                        .hashed_key(),
                                )
                            })
                            .collect();
                        for (addr, val) in fetch_storage_values(&pool, &keyed, to_block).await? {
                            if !val.is_zero() {
                                balances.push((addr, val));
                            }
                        }
                    }
                    balances
                }
            };

            eprintln!(
                "  [{}/{}] {}: {} holders.",
                i + 1,
                scan.len(),
                token.symbol,
                balances.len()
            );
            erc20_holders_seen += balances.len();
            if let Some(price) = token.price {
                for (addr, val) in balances {
                    let units = u256_to_f64(val, token.decimals);
                    *erc20_usd.entry(addr).or_default() += units * price;
                }
            }
        }
        eprintln!(
            "  scanned {erc20_holders_seen} (token,holder) balance slots; {} addresses with priced ERC20 value.",
            erc20_usd.len()
        );
    }

    // --- 4. Base token price + aggregate ---
    let base_price = if args.base_cg_id.is_empty() {
        None
    } else {
        fetch_simple_price(&client, &args, &args.base_cg_id).await
    };

    let mut all: HashSet<Address> = base_balance.keys().copied().collect();
    all.extend(erc20_usd.keys().copied());

    struct Holder {
        addr: Address,
        base: U256,
        base_usd: Option<f64>,
        erc20_usd: f64,
        total_usd: Option<f64>,
    }
    let mut holders: Vec<Holder> = all
        .into_iter()
        .map(|addr| {
            let base = base_balance.get(&addr).copied().unwrap_or_default();
            let base_usd = base_price.map(|p| u256_to_f64(base, args.base_decimals) * p);
            let erc = erc20_usd.get(&addr).copied().unwrap_or(0.0);
            let total_usd = match base_usd {
                Some(b) => Some(b + erc),
                None if erc > 0.0 => Some(erc),
                None => None,
            };
            Holder {
                addr,
                base,
                base_usd,
                erc20_usd: erc,
                total_usd,
            }
        })
        .collect();

    // Rank by total USD when available; otherwise by raw base-token balance.
    holders.sort_by(|a, b| match (a.total_usd, b.total_usd) {
        (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.base.cmp(&a.base),
    });

    if let Some(path) = &args.csv {
        use std::io::Write as _;
        let mut w = std::io::BufWriter::new(
            std::fs::File::create(path).with_context(|| format!("failed to create {path}"))?,
        );
        // All fields are numeric/hex, so no CSV escaping is needed.
        writeln!(
            w,
            "rank,address,{}_balance,base_usd,erc20_usd,total_usd,is_system",
            args.base_symbol.to_lowercase()
        )?;
        for (i, h) in holders.iter().enumerate() {
            writeln!(
                w,
                "{},{:?},{},{},{:.6},{},{}",
                i + 1,
                h.addr,
                format_units(h.base, args.base_decimals),
                h.base_usd.map(|v| format!("{v:.6}")).unwrap_or_default(),
                h.erc20_usd,
                h.total_usd.map(|v| format!("{v:.6}")).unwrap_or_default(),
                is_system_contract(&h.addr),
            )?;
        }
        w.flush()?;
        eprintln!("Wrote {} rows to {path}", holders.len());
    }

    println!(
        "\nSnapshot at block {to_block}. {} addresses with value (base price: {}).",
        holders.len(),
        base_price
            .map(|p| format!("${p:.2}"))
            .unwrap_or_else(|| "n/a".into())
    );
    println!(
        "{:<5} {:<44} {:>22} {:>16} {:>16} {:>16}",
        "RANK",
        "ADDRESS",
        format!("{}_BALANCE", args.base_symbol),
        "BASE_USD",
        "ERC20_USD",
        "TOTAL_USD"
    );
    println!("{}", "-".repeat(124));
    for (i, h) in holders.iter().take(args.top).enumerate() {
        let tag = if is_system_contract(&h.addr) {
            " (sys)"
        } else {
            ""
        };
        println!(
            "{:<5} {:<44} {:>22} {:>16} {:>16} {:>16}{tag}",
            i + 1,
            format!("{:?}", h.addr),
            format_units(h.base, args.base_decimals),
            opt_usd(h.base_usd),
            if h.erc20_usd > 0.0 {
                format!("{:.2}", h.erc20_usd)
            } else {
                "-".into()
            },
            opt_usd(h.total_usd),
        );
    }

    Ok(())
}

// --- DB scan helpers -----------------------------------------------------------

/// Distinct candidate addresses (tx initiator ∪ target) in `[start, end]`.
async fn enumerate_addresses_chunk(
    pool: &PgPool,
    start: u32,
    end: u32,
) -> anyhow::Result<Vec<Address>> {
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT addr
        FROM (
            SELECT UNNEST(ARRAY[initiator_address, contract_address]) AS addr
            FROM transactions
            WHERE miniblock_number BETWEEN $1 AND $2
        ) s
        WHERE addr IS NOT NULL
        "#,
    )
    .bind(i64::from(start))
    .bind(i64::from(end))
    .fetch_all(pool)
    .await
    .with_context(|| format!("address enumeration failed for {start}..={end}"))?;
    Ok(rows
        .iter()
        .filter_map(|row| row.try_get::<Vec<u8>, _>("addr").ok())
        .filter(|b| b.len() == 20)
        .map(|b| Address::from_slice(&b))
        .collect())
}

/// Looks up the latest value for each (address, hashed_key) as of `block`. Returns
/// (address, balance) pairs. Mirrors the node's "latest value <= block" semantics.
async fn fetch_storage_values(
    pool: &PgPool,
    keyed: &[(Address, H256)],
    block: u32,
) -> anyhow::Result<Vec<(Address, U256)>> {
    let keys: Vec<Vec<u8>> = keyed.iter().map(|(_, k)| k.as_bytes().to_vec()).collect();
    // hashed_key -> address (keys are unique per address within a token context).
    let by_key: HashMap<Vec<u8>, Address> = keyed
        .iter()
        .map(|(a, k)| (k.as_bytes().to_vec(), *a))
        .collect();

    let rows = sqlx::query(
        r#"
        SELECT
            u.hashed_key AS hashed_key,
            (
                SELECT value FROM storage_logs
                WHERE hashed_key = u.hashed_key AND miniblock_number <= $2
                ORDER BY miniblock_number DESC, operation_number DESC
                LIMIT 1
            ) AS value
        FROM UNNEST($1::bytea[]) AS u (hashed_key)
        "#,
    )
    .bind(&keys)
    .bind(i64::from(block))
    .fetch_all(pool)
    .await
    .context("storage_logs batch lookup failed")?;

    let mut out = Vec::new();
    for row in &rows {
        let hk: Vec<u8> = row.try_get("hashed_key")?;
        let val: Option<Vec<u8>> = row.try_get("value")?;
        if let (Some(addr), Some(bytes)) = (by_key.get(&hk), val) {
            out.push((*addr, U256::from_big_endian(&bytes)));
        }
    }
    Ok(out)
}

async fn load_tokens(pool: &PgPool, filter: &[Address]) -> anyhow::Result<Vec<Token>> {
    let base = "SELECT l1_address, l2_address, symbol, decimals FROM tokens";
    let sql = if filter.is_empty() {
        base.to_string()
    } else {
        format!("{base} WHERE l1_address = ANY($1::bytea[])")
    };
    let mut query = sqlx::query(&sql);
    if !filter.is_empty() {
        let addrs: Vec<Vec<u8>> = filter.iter().map(|a| a.as_bytes().to_vec()).collect();
        query = query.bind(addrs);
    }
    query
        .fetch_all(pool)
        .await
        .context("failed to read tokens table")?
        .into_iter()
        .map(|row| {
            let l1: Vec<u8> = row.try_get("l1_address")?;
            let l2: Vec<u8> = row.try_get("l2_address")?;
            anyhow::Ok(Token {
                l1_address: Address::from_slice(&l1),
                l2_address: Address::from_slice(&l2),
                symbol: row.try_get("symbol")?,
                decimals: row.try_get::<i32, _>("decimals")? as u32,
            })
        })
        .collect()
}

// --- on-disk checkpoint cache --------------------------------------------------

/// Pinned block window for a cache directory (keeps the snapshot stable on resume).
struct Meta {
    from_block: u32,
    to_block: u32,
}

/// Layout of the `--cache-dir` checkpoint directory. Every long phase keeps an
/// append-only data file plus a `*.cursor` counter so it can resume mid-phase. All
/// files store raw balances (pricing is re-applied in-process), so a stale cache
/// never bakes in an outdated price. Cursors are advanced only *after* the
/// corresponding data is flushed+fsynced, so a crash re-does at most one chunk; any
/// duplicate rows that produces are deduped on load.
struct Cache {
    dir: PathBuf,
    meta: PathBuf,
    accounts: PathBuf,
    accounts_cursor: PathBuf,
    base_balances: PathBuf,
    base_cursor: PathBuf,
    tokens_dir: PathBuf,
}

impl Cache {
    fn new(dir: &str) -> Self {
        let dir = PathBuf::from(dir);
        Cache {
            meta: dir.join("meta.txt"),
            accounts: dir.join("accounts.txt"),
            accounts_cursor: dir.join("accounts.cursor"),
            base_balances: dir.join("base_balances.csv"),
            base_cursor: dir.join("base_balances.cursor"),
            tokens_dir: dir.join("tokens"),
            dir,
        }
    }

    fn init(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.tokens_dir)
            .with_context(|| format!("failed to create {}", self.tokens_dir.display()))?;
        Ok(())
    }

    /// Per-token files: raw balances, the balance-read cursor (an index into the
    /// candidate address list), and a `.done` marker written once a token's balances
    /// are fully materialized.
    fn token_path(&self, l2: Address) -> PathBuf {
        self.tokens_dir.join(format!("{l2:?}.csv"))
    }
    fn token_bcursor(&self, l2: Address) -> PathBuf {
        self.tokens_dir.join(format!("{l2:?}.cursor"))
    }
    fn token_done(&self, l2: Address) -> PathBuf {
        self.tokens_dir.join(format!("{l2:?}.done"))
    }

    fn load_meta(&self) -> anyhow::Result<Option<Meta>> {
        if !self.meta.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&self.meta)
            .with_context(|| format!("failed to read {}", self.meta.display()))?;
        let mut from_block = None;
        let mut to_block = None;
        for line in text.lines() {
            if let Some(v) = line.trim().strip_prefix("from_block=") {
                from_block = v.trim().parse().ok();
            } else if let Some(v) = line.trim().strip_prefix("to_block=") {
                to_block = v.trim().parse().ok();
            }
        }
        match (from_block, to_block) {
            (Some(from_block), Some(to_block)) => Ok(Some(Meta {
                from_block,
                to_block,
            })),
            _ => anyhow::bail!("malformed {}", self.meta.display()),
        }
    }

    fn save_meta(&self, m: &Meta) -> anyhow::Result<()> {
        let body = format!("from_block={}\nto_block={}\n", m.from_block, m.to_block);
        write_atomic(&self.meta, body.as_bytes())
    }
}

/// Reads `0x...` hex addresses, one per line, preserving file order and deduping.
fn read_addresses(path: &Path) -> anyhow::Result<Vec<Address>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let addr = parse_address(line)?;
        if seen.insert(addr) {
            out.push(addr);
        }
    }
    Ok(out)
}

/// Reads `addr,raw_u256` lines, deduped (last value wins) so a re-run that re-appended
/// a chunk after a crash never double-counts. Missing file = empty.
fn read_balances(path: &Path) -> anyhow::Result<Vec<(Address, U256)>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut map: HashMap<Address, U256> = HashMap::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (addr, val) = line
            .split_once(',')
            .with_context(|| format!("malformed balance line in {}: {line}", path.display()))?;
        let addr = parse_address(addr)?;
        let val = U256::from_dec_str(val.trim())
            .with_context(|| format!("invalid balance in {}: {val}", path.display()))?;
        map.insert(addr, val);
    }
    Ok(map.into_iter().collect())
}

fn addr_lines(addrs: &[Address]) -> String {
    addrs.iter().map(|a| format!("{a:?}\n")).collect()
}

fn balance_lines(balances: &[(Address, U256)]) -> String {
    balances
        .iter()
        .map(|(a, v)| format!("{a:?},{v}\n"))
        .collect()
}

/// Reads a cursor counter; 0 if the file is missing or unparsable.
fn read_cursor(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn write_cursor(path: &Path, n: usize) -> anyhow::Result<()> {
    write_atomic(path, n.to_string().as_bytes())
}

/// Appends `body` to `path` (creating it), then fsyncs so the bytes are durable
/// before the caller advances its cursor.
fn append(path: &Path, body: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    if body.is_empty() {
        return Ok(());
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {} for append", path.display()))?;
    f.write_all(body.as_bytes())?;
    f.sync_all()
        .with_context(|| format!("failed to fsync {}", path.display()))?;
    Ok(())
}

/// Writes `bytes` to `path` via a sibling temp file + rename (atomic on the same
/// filesystem), so readers only ever see a fully-written file.
fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write as _;
    let tmp = path.with_extension("tmp");
    {
        let mut w = std::fs::File::create(&tmp)
            .with_context(|| format!("failed to create {}", tmp.display()))?;
        w.write_all(bytes)?;
        w.sync_all()?;
    }
    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

// --- CoinGecko pricing (same shape as l1_token_balances) -----------------------

fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("zksync-era-treasury-analytics/1.0")
        .build()
        .context("failed to build HTTP client")
}

fn with_api_key(req: reqwest::RequestBuilder, args: &Args) -> reqwest::RequestBuilder {
    match &args.coingecko_api_key {
        Some(key) if args.coingecko_base_url.contains("pro-api") => {
            req.header("x-cg-pro-api-key", key)
        }
        Some(key) => req.header("x-cg-demo-api-key", key),
        None => req,
    }
}

async fn fetch_token_prices(
    client: &reqwest::Client,
    args: &Args,
    addrs: &[Address],
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    let (batch, sleep_ms) = if args.coingecko_api_key.is_some() {
        (PRICE_BATCH_WITH_KEY, 250)
    } else {
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
        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if !status.is_success() {
                    eprintln!("  CoinGecko: HTTP {status}: {}", truncate(body.trim(), 200));
                } else if let Ok(map) = serde_json::from_str::<HashMap<String, PriceEntry>>(&body) {
                    for (addr, entry) in map {
                        if let Some(usd) = entry.usd {
                            out.insert(addr.to_lowercase(), usd);
                        }
                    }
                }
            }
            Err(err) => eprintln!("  CoinGecko: price request failed: {err:#}"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
    }
    out
}

async fn fetch_simple_price(client: &reqwest::Client, args: &Args, id: &str) -> Option<f64> {
    let url = format!("{}/simple/price", args.coingecko_base_url);
    let req = with_api_key(client.get(&url), args).query(&[("ids", id), ("vs_currencies", "usd")]);
    let resp = req.send().await.ok()?;
    if !resp.status().is_success() {
        eprintln!("  CoinGecko: base price HTTP {}", resp.status());
        return None;
    }
    let map = resp.json::<HashMap<String, PriceEntry>>().await.ok()?;
    map.get(id).and_then(|e| e.usd)
}

#[derive(serde::Deserialize)]
struct PriceEntry {
    usd: Option<f64>,
}

// --- helpers -------------------------------------------------------------------

/// Block ranges covering `[from, to]`, ordered latest-first (each tuple is itself
/// ascending). Scanning newest blocks first means an early stop on `--max-addresses`
/// keeps the most recently active addresses.
fn chunks(from: u32, to: u32, size: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut end = to;
    loop {
        let start = end.saturating_sub(size - 1).max(from);
        out.push((start, end));
        if start <= from {
            break;
        }
        end = start - 1;
    }
    out
}

fn addr_key(addr: Address) -> String {
    format!("{addr:?}")
}

fn parse_address(s: &str) -> anyhow::Result<Address> {
    let s = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    let bytes = hex::decode(s).with_context(|| format!("invalid hex address: {s}"))?;
    anyhow::ensure!(bytes.len() == 20, "address must be 20 bytes: {s}");
    Ok(Address::from_slice(&bytes))
}

fn opt_usd(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.2}")).unwrap_or_else(|| "n/a".into())
}

/// zkEVM system contracts live at small addresses (high 18 bytes are zero).
fn is_system_contract(addr: &Address) -> bool {
    addr.as_bytes()[..18].iter().all(|&b| b == 0) && !addr.is_zero()
}

fn u256_to_f64(value: U256, decimals: u32) -> f64 {
    format_units(value, decimals).parse().unwrap_or(0.0)
}

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
    let frac = frac_part.trim_end_matches('0');
    if frac.is_empty() {
        int_part
    } else {
        format!("{int_part}.{frac}")
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}
