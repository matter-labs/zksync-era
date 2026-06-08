# treasury_analytics

Two one-off analytics scripts.

Build (from `core/`):

```bash
cargo build --release --bin l1_token_balances --bin hot_contracts
```

## `l1_token_balances` — L1 funds held per token

Reads the canonical token list from the node's `tokens` table, then queries an L1 RPC for the balance the **L1 Shared
Bridge / Asset Router** escrows for each token (`balanceOf`, plus `eth_getBalance` for ETH, which has the zero L1
address).

USD prices are fetched **live from CoinGecko** keyed by L1 contract address (the DB `usd_price` column is unpopulated in
practice). Prices are requested in batches and used to value + rank the holdings; tokens CoinGecko doesn't list show
`n/a`.

```bash
DATABASE_URL=postgres://... \
L1_RPC_URL=https://eth-mainnet.example/... \
SHARED_BRIDGE_ADDR=0x399b8787980a3da54a5f4358752c82be648d6d6a \
COINGECKO_API_KEY=...   # optional, raises rate limits \
./target/release/l1_token_balances
```

The shared bridge address is the `bridges.shared.l1_address` field in the node's contracts config
(`configs/contracts.yaml`), or env `CONTRACTS_BRIDGES_SHARED_L1_ADDRESS` / `L1_SHARED_BRIDGE_PROXY_ADDR` depending on
the deployment.

Flags:

- `--well-known-only` — skip tokens not flagged `well_known`.
- `--coingecko-api-key` (env `COINGECKO_API_KEY`) — optional; demo vs pro is inferred from the base URL.
- `--coingecko-base-url` (env `COINGECKO_BASE_URL`) — point at the pro endpoint if you have a pro key (default:
  `https://api.coingecko.com/api/v3`).

## `hot_contracts` — hottest contracts by transaction count

Aggregates the `transactions` table by `contract_address` (the execute-tx target). This counts **top-level transaction
targets only** — internal sub-calls are not included.

The table can be huge, so it is **never** scanned in one shot: the script walks the block range in bounded chunks (each
query is constrained to `--chunk` blocks via the `transactions (miniblock_number, ...)` index) and merges the per-chunk
`GROUP BY` results into an in-memory tally.

```bash
DATABASE_URL=postgres://... \
./target/release/hot_contracts --last 50000 --top 50
```

Flags:

- `--from-block` / `--to-block` — explicit L2 block (miniblock) range.
- `--last N` — scan the last N blocks (default 50000) when `--from-block` is omitted.
- `--top N` — rows to print (default 50).
- `--chunk N` — blocks per DB query (default 10000); lower it if individual chunks are still too heavy.

Transactions without a target (`contract_address IS NULL`, e.g. deployments) are excluded from the ranking but reported
in the header line.

## `rich_contracts` — L2 addresses holding the most value

Ranks L2 addresses (EOAs **and** contracts) by total value held: base token (ETH) plus ERC20 holdings, priced via
CoinGecko.

Balances are read from **state** (`storage_logs`), not RPC — the exact Blake2s256 storage keys are computed with
`zksync_types` (same as the node's `eth_getBalance`).

- Candidate addresses come from `transactions` (initiator ∪ target), chunked.
- For each scanned token, the `balanceOf` slot is read directly for every candidate address (suited to a small, fixed
  token set — no `events` scan). **Caveat:** an address that only ever _received_ a token, without initiating or being
  the target of a transaction, is not a candidate and is therefore missed.

```bash
DATABASE_URL=postgres://... COINGECKO_API_KEY=... \
./target/release/rich_contracts --top 50
```

On a large chain this is a multi-hour/multi-day job, so it can checkpoint to disk **continuously** and resume mid-phase:

- `--cache-dir DIR` — persist progress under `DIR`. Every block chunk and storage batch is flushed (and fsynced) as it
  completes and a cursor advanced, so the process can be stopped at any moment (Ctrl-C, OOM, power loss) and a re-run
  with the same `--cache-dir` picks up where it left off — redoing at most the last in-flight chunk. Files written:

  - `meta.txt` — the **pinned** block window. Reused on every resume so base-token and ERC20 balances stay a consistent
    snapshot even as the chain advances.
  - `accounts.txt` (+ `accounts.cursor`) — candidate address universe.
  - `base_balances.csv` (+ `base_balances.cursor`) — base-token balances.
  - `tokens/<l2addr>.csv` (+ `.cursor`, `.done`) — per-token raw balances; the `.cursor` is an index into the candidate
    address list and `.done` marks a fully-scanned token so resumes skip it.

  Balances are stored raw, so pricing is always re-fetched fresh. Delete a file (or the whole dir) to force that part to
  recompute.

Flags: `--from-block`/`--to-block`, `--chunk`, `--storage-batch`, `--max-addresses` (safety cap), `--no-erc20` (base
token only), `--base-decimals`/`--base-symbol`/ `--base-cg-id` (for non-ETH base tokens).

- `--tokens` — restrict ERC20s to these L1 token addresses (comma-separated). The base token (zero address) is always
  included.
- `--extra-tokens` — L2-native tokens not in the `tokens` table (e.g. ZK), each `L2ADDR[:COINGECKO_ID[:DECIMALS]]`. The
  coin id prices it via `/simple/price`. Example: `--extra-tokens 0x5a7d…eaf3e:zksync:18`.

**Caveats:**

- ERC20 balances are correct only for zkSync's _standard_ ERC20 layout (`balanceOf` at storage slot 51). Custom-layout
  tokens will be misread.
- Pricing is CoinGecko-by-L1-address; on testnet/stage those addresses aren't listed, so ERC20 USD ≈ 0 and ranking
  effectively falls back to the (priced) base token.
- On pruned chains only retained `storage_logs` are visible.
