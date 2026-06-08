//! Ranks the "hottest" contracts by how many transactions target them, using the
//! `transactions` table directly (the `contract_address` column is the execute-tx
//! target). This counts only top-level transaction targets — not internal calls.
//!
//! The `transactions` table can be enormous, so we never run a single aggregation
//! over the whole range. Instead we scan in bounded block chunks (each query is
//! constrained to `--chunk` blocks via the `miniblock_number` index) and merge the
//! per-chunk `GROUP BY` results into an in-memory tally.
//!
//! Usage:
//!   DATABASE_URL=postgres://... \
//!   cargo run --release --bin hot_contracts -- --last 50000 --top 50

use std::collections::{BTreeMap, HashMap};

use anyhow::Context as _;
use clap::Parser;
use sqlx::{postgres::PgPoolOptions, Row};
use zksync_types::Address;

#[derive(Debug, Parser)]
#[command(about = "Rank hottest contracts by transaction count")]
struct Args {
    /// Postgres connection string for the node DB.
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    /// First L2 block (miniblock) to scan. Defaults to `to_block - last + 1`.
    #[arg(long)]
    from_block: Option<u32>,
    /// Last L2 block (miniblock) to scan. Defaults to the latest sealed block.
    #[arg(long)]
    to_block: Option<u32>,
    /// How many recent blocks to scan when `--from-block` is omitted.
    #[arg(long, default_value_t = 50_000)]
    last: u32,
    /// How many top contracts to print.
    #[arg(long, default_value_t = 50)]
    top: usize,
    /// Blocks per DB query. Each chunk is aggregated server-side and merged locally,
    /// keeping every query bounded so a huge table is never scanned at once.
    #[arg(long, default_value_t = 100_000)]
    chunk: u32,
    /// Break the ranking down by calendar month (UTC), printing a top-N per month.
    #[arg(long)]
    monthly: bool,
}

#[derive(Default)]
struct Bucket {
    counts: HashMap<Address, u64>,
    total: u64,
    no_target: u64,
}

impl Bucket {
    fn add(&mut self, contract: Option<&[u8]>, cnt: u64) {
        self.total += cnt;
        match contract {
            Some(bytes) => *self.counts.entry(Address::from_slice(bytes)).or_default() += cnt,
            None => self.no_target += cnt,
        }
    }

    fn print(&self, title: &str, top: usize) {
        let mut ranked: Vec<(&Address, &u64)> = self.counts.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1));
        println!(
            "\n{title}: {} transactions ({} without a target / deployments), {} unique contracts.",
            self.total,
            self.no_target,
            self.counts.len()
        );
        println!(
            "{:<5} {:<44} {:>14} {:>8}",
            "RANK", "ADDRESS", "TXS", "SHARE"
        );
        println!("{}", "-".repeat(76));
        for (i, (addr, count)) in ranked.iter().take(top).enumerate() {
            let share = if self.total > 0 {
                **count as f64 / self.total as f64 * 100.0
            } else {
                0.0
            };
            let tag = if is_system_contract(addr) {
                "  (system)"
            } else {
                ""
            };
            println!(
                "{:<5} {:<44} {:>14} {:>7.2}%{tag}",
                i + 1,
                format!("{addr:?}"),
                count,
                share
            );
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    anyhow::ensure!(args.chunk > 0, "--chunk must be > 0");

    let pool = PgPoolOptions::new()
        .max_connections(2)
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
    let from_block = args
        .from_block
        .unwrap_or_else(|| to_block.saturating_sub(args.last.saturating_sub(1)));

    eprintln!(
        "Aggregating transactions for blocks {from_block}..={to_block} in chunks of {} blocks...",
        args.chunk
    );

    // Single overall bucket, or one bucket per "YYYY-MM" (UTC) when --monthly.
    let mut overall = Bucket::default();
    let mut monthly: BTreeMap<String, Bucket> = BTreeMap::new();

    let mut start = from_block;
    loop {
        let end = start.saturating_add(args.chunk - 1).min(to_block);

        // One bounded, index-backed aggregation per chunk. NULL `contract_address`
        // (deployments / no target) comes back as its own group.
        let sql = if args.monthly {
            r#"
            SELECT
                TO_CHAR(TO_TIMESTAMP(m.timestamp) AT TIME ZONE 'UTC', 'YYYY-MM') AS month,
                t.contract_address AS contract_address,
                COUNT(*) AS cnt
            FROM transactions t
            JOIN miniblocks m ON t.miniblock_number = m.number
            WHERE t.miniblock_number BETWEEN $1 AND $2
            GROUP BY month, t.contract_address
            "#
        } else {
            r#"
            SELECT
                NULL AS month,
                contract_address,
                COUNT(*) AS cnt
            FROM transactions
            WHERE miniblock_number BETWEEN $1 AND $2
            GROUP BY contract_address
            "#
        };
        let rows = sqlx::query(sql)
            .bind(i64::from(start))
            .bind(i64::from(end))
            .fetch_all(&pool)
            .await
            .with_context(|| format!("aggregation query failed for blocks {start}..={end}"))?;

        for row in &rows {
            let cnt = row.try_get::<i64, _>("cnt")? as u64;
            let contract = row.try_get::<Option<Vec<u8>>, _>("contract_address")?;
            overall.add(contract.as_deref(), cnt);
            if args.monthly {
                let month: String = row.try_get("month")?;
                monthly
                    .entry(month)
                    .or_default()
                    .add(contract.as_deref(), cnt);
            }
        }

        eprintln!("  ..{end}: {} txs so far", overall.total);

        if end == to_block {
            break;
        }
        start = end.saturating_add(1);
    }

    println!("\nScanned blocks {from_block}..={to_block}.");
    if args.monthly {
        for (month, bucket) in &monthly {
            bucket.print(month, args.top);
        }
    } else {
        overall.print("Overall", args.top);
    }

    Ok(())
}

/// zkEVM system contracts live at small addresses (high 18 bytes are zero).
fn is_system_contract(addr: &Address) -> bool {
    let bytes = addr.as_bytes();
    bytes[..18].iter().all(|&b| b == 0) && !addr.is_zero()
}
