//! Heuristically lists ERC20 tokens that exist ONLY on L2 (never bridged from L1).
//!
//! The node's `tokens` table cannot answer this: `l1_address` is `NOT NULL` and is the
//! primary key, and the table is populated solely from `BridgeInitialize` events, so it
//! effectively enumerates L1-originated tokens. Instead we derive the answer from the
//! `events` table:
//!
//!   candidates = {ERC20-shaped Transfer emitters}  MINUS  {bridged tokens}
//!
//! * ERC20-shaped: a contract that emitted `Transfer(address,address,uint256)` with an
//!   empty 4th topic. ERC721 indexes `tokenId` into topic4, so any contract that ever
//!   emitted a Transfer with a non-zero topic4 is treated as NFT-like and dropped.
//! * bridged: deployed by the L2NativeTokenVault (a ContractDeployer `DEPLOY` event whose
//!   deployer topic is the NTV), OR emitted `BridgeInitialize`/`BridgeInitialization`, OR
//!   already present in the `tokens` table with a non-zero L1 address.
//!
//! ## Not overwhelming the DB
//!
//! The `events` table is enormous, so the Transfer scan is **never** run in one shot: it
//! walks the block range in bounded chunks (`--chunk`), aggregates each chunk server-side
//! (`GROUP BY address`), and merges per-address flags in memory (the number of distinct
//! token contracts is small even though raw Transfer rows are in the billions). Each
//! connection sets a `statement_timeout`, so a runaway chunk is aborted rather than left
//! grinding. The pool is capped at 2 connections and every query runs sequentially. The
//! bridged-set queries run once over all history but are index-selective (`topic1` /
//! `topic2`), not full scans.
//!
//! ## Caveats (this is a heuristic)
//!
//! * A custom bridge that neither deploys via the NTV nor emits a `BridgeInitialize`
//!   event would slip through and look L2-native.
//! * An NFT collection that only ever moved `tokenId` 0 would be misclassified as ERC20.
//! * A native token that has never emitted a Transfer is invisible (but it has no
//!   balances to report anyway).
//!
//! For certainty on a specific candidate, read the L2NativeTokenVault `originChainId`
//! storage for its asset id (`== this chain id` ⇒ native).
//!
//! Usage:
//!   DATABASE_URL=postgres://... \
//!   cargo run --release --bin l2_native_tokens -- --top 100 --csv candidates.csv

use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use clap::Parser;
use sqlx::{postgres::PgPoolOptions, Executor, Row};
use zksync_system_constants::{CONTRACT_DEPLOYER_ADDRESS, L2_NATIVE_TOKEN_VAULT_ADDRESS};
use zksync_types::{ethabi, Address};

/// `VmEvent::DEPLOY_EVENT_SIGNATURE` (the `ContractDeployed` event). Hardcoded here to
/// avoid pulling in `zksync_multivm` for a single constant; see
/// `core/lib/vm_interface/src/types/outputs/execution_result.rs`.
const DEPLOY_EVENT_SIGNATURE: [u8; 32] = [
    41, 10, 253, 174, 35, 26, 63, 192, 187, 174, 139, 26, 246, 54, 152, 176, 161, 215, 155, 33,
    173, 23, 223, 3, 66, 223, 185, 82, 254, 116, 248, 229,
];

#[derive(Debug, Parser)]
#[command(about = "Heuristically list ERC20 tokens that exist only on L2 (never bridged from L1)")]
struct Args {
    /// Postgres connection string for the node DB.
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    /// First L2 block (miniblock) to scan for Transfer activity. Default: 0 (genesis).
    #[arg(long)]
    from_block: Option<u32>,
    /// Last L2 block (miniblock) to scan. Default: the latest sealed block.
    #[arg(long)]
    to_block: Option<u32>,
    /// Scan only the last N blocks (sets `from_block = to_block - N + 1`). Omit for full
    /// history. Ignored if `--from-block` is given.
    #[arg(long)]
    last: Option<u32>,
    /// Blocks per DB query for the Transfer scan. Each chunk is aggregated server-side and
    /// merged locally, so a huge table is never scanned at once. Lower it if chunks are
    /// still too heavy.
    #[arg(long, default_value_t = 100_000)]
    chunk: u32,
    /// Per-query statement timeout (seconds). A chunk that exceeds this is aborted instead
    /// of grinding indefinitely. Set 0 to disable.
    #[arg(long, default_value_t = 300)]
    statement_timeout_secs: u64,
    /// How many candidates to print (ranked by transfer count).
    #[arg(long, default_value_t = 100)]
    top: usize,
    /// Write all candidates to this CSV (`address,transfers,in_tokens_table,symbol`).
    #[arg(long)]
    csv: Option<String>,
}

/// Per-contract accumulator merged across chunks.
#[derive(Default)]
struct Agg {
    transfers: u64,
    /// Saw at least one Transfer with an empty topic4 (ERC20-shaped).
    erc20_like: bool,
    /// Saw at least one Transfer with a non-zero topic4 (ERC721-shaped).
    erc721_like: bool,
}

/// Left-pad a 20-byte address into a 32-byte topic value.
fn address_topic(addr: &Address) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(addr.as_bytes());
    out
}

/// Extract a 20-byte address from a 32-byte (left-padded) topic / address column.
fn addr_from_bytes(bytes: &[u8]) -> Address {
    Address::from_slice(&bytes[bytes.len().saturating_sub(20)..])
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    anyhow::ensure!(args.chunk > 0, "--chunk must be > 0");

    let timeout_ms = args.statement_timeout_secs.saturating_mul(1000);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                // Cap every query on this connection so a runaway chunk can't grind forever.
                conn.execute(format!("SET statement_timeout = {timeout_ms}").as_str())
                    .await?;
                Ok(())
            })
        })
        .connect(&args.database_url)
        .await
        .context("failed to connect to Postgres")?;

    let to_block = match args.to_block {
        Some(b) => b,
        None => {
            let row = sqlx::query("SELECT MAX(number) AS max FROM miniblocks")
                .fetch_one(&pool)
                .await?;
            row.try_get::<Option<i64>, _>("max")?
                .context("no miniblocks in DB")? as u32
        }
    };
    let from_block = match (args.from_block, args.last) {
        (Some(f), _) => f,
        (None, Some(last)) => to_block.saturating_sub(last.saturating_sub(1)),
        (None, None) => 0,
    };

    // Event signatures (the topic1 values we filter on).
    let transfer_sig = ethabi::long_signature(
        "Transfer",
        &[
            ethabi::ParamType::Address,
            ethabi::ParamType::Address,
            ethabi::ParamType::Uint(256),
        ],
    );
    let bridge_init_new = ethabi::long_signature(
        "BridgeInitialize",
        &[
            ethabi::ParamType::Address,
            ethabi::ParamType::String,
            ethabi::ParamType::String,
            ethabi::ParamType::Uint(8),
        ],
    );
    let bridge_init_old = ethabi::long_signature(
        "BridgeInitialization",
        &[
            ethabi::ParamType::Address,
            ethabi::ParamType::String,
            ethabi::ParamType::String,
            ethabi::ParamType::Uint(8),
        ],
    );
    let ntv_topic = address_topic(&L2_NATIVE_TOKEN_VAULT_ADDRESS);
    let zero32 = [0u8; 32];

    // ---- Phase 1: build the bridged-token exclusion set (cheap, index-selective). ----
    eprintln!("Building bridged-token exclusion set (all history)...");
    let mut bridged: HashSet<Address> = HashSet::new();

    // (a) Tokens whose beacon proxy was deployed by the L2NativeTokenVault. The
    // ContractDeployer DEPLOY event records the deployer (msg.sender) in topic2 and the
    // deployed address in topic4. `topic2 = NTV` is highly selective via events_topic2_idx.
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT topic4
        FROM events
        WHERE topic1 = $1 AND address = $2 AND topic2 = $3
        "#,
    )
    .bind(DEPLOY_EVENT_SIGNATURE.as_slice())
    .bind(CONTRACT_DEPLOYER_ADDRESS.as_bytes())
    .bind(ntv_topic.as_slice())
    .fetch_all(&pool)
    .await
    .context("NTV-deployed-tokens query failed")?;
    for row in &rows {
        let t: Vec<u8> = row.try_get("topic4")?;
        bridged.insert(addr_from_bytes(&t));
    }
    let ntv_deployed = bridged.len();

    // (b) Tokens that emitted a BridgeInitialize / (legacy) BridgeInitialization event.
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT address
        FROM events
        WHERE topic1 = $1 OR topic1 = $2
        "#,
    )
    .bind(bridge_init_new.as_bytes())
    .bind(bridge_init_old.as_bytes())
    .fetch_all(&pool)
    .await
    .context("bridge-init query failed")?;
    for row in &rows {
        let a: Vec<u8> = row.try_get("address")?;
        bridged.insert(addr_from_bytes(&a));
    }

    // (c) Anything the node already records as an L1-originated token (non-zero l1_address).
    // Also remember symbols so we can annotate / flag any heuristic disagreement.
    let mut known_symbol: HashMap<Address, String> = HashMap::new();
    let rows = sqlx::query("SELECT l2_address, l1_address, symbol FROM tokens")
        .fetch_all(&pool)
        .await
        .context("tokens-table query failed")?;
    for row in &rows {
        let l2 = addr_from_bytes(&row.try_get::<Vec<u8>, _>("l2_address")?);
        let l1 = addr_from_bytes(&row.try_get::<Vec<u8>, _>("l1_address")?);
        if let Ok(Some(sym)) = row.try_get::<Option<String>, _>("symbol") {
            known_symbol.insert(l2, sym);
        }
        if !l1.is_zero() {
            bridged.insert(l2);
        }
    }

    // ---- Phase 2: harvest ERC20-shaped Transfer emitters, chunked over the block range. ----
    eprintln!(
        "Scanning Transfer events for blocks {from_block}..={to_block} in chunks of {} blocks...",
        args.chunk
    );
    let mut agg: HashMap<Address, Agg> = HashMap::new();

    let mut start = from_block;
    loop {
        let end = start.saturating_add(args.chunk - 1).min(to_block);

        // One bounded, server-side aggregation per chunk: for each emitting contract, did
        // it ever look ERC20 (empty topic4) and/or ERC721 (non-empty topic4) in this range.
        let rows = sqlx::query(
            r#"
            SELECT
                address,
                COUNT(*) AS cnt,
                bool_or(topic4 = $1) AS erc20_like,
                bool_or(topic4 <> $1) AS erc721_like
            FROM events
            WHERE topic1 = $2 AND miniblock_number BETWEEN $3 AND $4
            GROUP BY address
            "#,
        )
        .bind(zero32.as_slice())
        .bind(transfer_sig.as_bytes())
        .bind(i64::from(start))
        .bind(i64::from(end))
        .fetch_all(&pool)
        .await
        .with_context(|| format!("Transfer scan failed for blocks {start}..={end}"))?;

        for row in &rows {
            let addr = addr_from_bytes(&row.try_get::<Vec<u8>, _>("address")?);
            let cnt = row.try_get::<i64, _>("cnt")? as u64;
            let e20 = row
                .try_get::<Option<bool>, _>("erc20_like")?
                .unwrap_or(false);
            let e721 = row
                .try_get::<Option<bool>, _>("erc721_like")?
                .unwrap_or(false);
            let e = agg.entry(addr).or_default();
            e.transfers += cnt;
            e.erc20_like |= e20;
            e.erc721_like |= e721;
        }

        eprintln!(
            "  ..{end}: {} distinct transfer-emitting contracts so far",
            agg.len()
        );

        if end == to_block {
            break;
        }
        start = end.saturating_add(1);
    }

    // ---- Classify ----
    let total_emitters = agg.len();
    let mut nft_like = 0usize;
    let mut excluded_bridged = 0usize;
    let mut candidates: Vec<(Address, u64)> = Vec::new();
    for (addr, a) in &agg {
        if a.erc721_like || !a.erc20_like {
            nft_like += 1;
            continue;
        }
        if bridged.contains(addr) {
            excluded_bridged += 1;
            continue;
        }
        candidates.push((*addr, a.transfers));
    }
    candidates.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));

    // ---- Report ----
    println!("\nScanned blocks {from_block}..={to_block}.");
    println!(
        "Bridged exclusion set: {} addresses ({ntv_deployed} deployed by NTV, plus \
         BridgeInitialize emitters and non-ETH tokens-table entries).",
        bridged.len()
    );
    println!("Distinct Transfer-emitting contracts: {total_emitters}");
    println!("  dropped as NFT-like / non-ERC20:    {nft_like}");
    println!("  dropped as bridged-from-L1:          {excluded_bridged}");
    println!(
        "  => L2-native ERC20 candidates:       {}",
        candidates.len()
    );

    println!(
        "\n{:<5} {:<44} {:>14} {:>6}  SYMBOL",
        "RANK", "ADDRESS", "TRANSFERS", "IN_DB"
    );
    println!("{}", "-".repeat(86));
    for (i, (addr, transfers)) in candidates.iter().take(args.top).enumerate() {
        let sym = known_symbol.get(addr);
        println!(
            "{:<5} {:<44} {:>14} {:>6}  {}",
            i + 1,
            format!("{addr:?}"),
            transfers,
            if sym.is_some() { "yes" } else { "no" },
            sym.map(String::as_str).unwrap_or("")
        );
    }
    // A candidate that IS in the tokens table is a heuristic disagreement worth a look
    // (the tokens table is meant to hold L1-originated tokens).
    let in_db = candidates
        .iter()
        .filter(|(a, _)| known_symbol.contains_key(a))
        .count();
    if in_db > 0 {
        println!(
            "\nNote: {in_db} candidate(s) also appear in the `tokens` table — inspect these, \
             the heuristic and the node's token list disagree."
        );
    }

    if let Some(path) = &args.csv {
        use std::io::Write as _;
        let mut f = std::fs::File::create(path).with_context(|| format!("create {path}"))?;
        writeln!(f, "address,transfers,in_tokens_table,symbol")?;
        for (addr, transfers) in &candidates {
            let sym = known_symbol.get(addr).map(String::as_str).unwrap_or("");
            writeln!(
                f,
                "{addr:?},{transfers},{},{sym}",
                known_symbol.contains_key(addr)
            )?;
        }
        eprintln!("Wrote {} candidates to {path}", candidates.len());
    }

    Ok(())
}
