//! `batch_splitter` — split a sealed L1 batch into two independently-provable
//! Airbender pieces. See `README.md` for the design and constraints.

use std::path::PathBuf;

use anyhow::Context as _;
use clap::Parser;
use zksync_config::ObjectStoreConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStoreFactory;
use zksync_types::{url::SensitiveUrl, L1BatchNumber, L2ChainId};

mod blob;
mod commitment;
mod reexec;
mod sparse_tree;
mod split;

use crate::{
    blob::{Half, SplitPiece},
    split::{split, SplitParams},
};

#[derive(Debug, Parser)]
#[command(
    name = "batch_splitter",
    about = "Split a sealed L1 batch into two independently-provable Airbender pieces"
)]
struct Cli {
    /// L1 batch number to split.
    #[arg(long)]
    l1_batch: u32,

    /// Postgres URL of the node (read-only use).
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Base path of the file-backed object store holding the batch's witness
    /// blobs (the witness-input bucket). The two pieces are written back here.
    #[arg(long)]
    artifacts_path: PathBuf,

    /// L2 chain id.
    #[arg(long)]
    l2_chain_id: u64,

    /// Compute piece A's commitment to chain into piece B's commitment_input.
    /// Off by default: pieces are produced for geometry-feasibility checks with
    /// `commitment_input = None` for B (see README).
    #[arg(long, default_value_t = false)]
    commitment_chaining: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let batch = L1BatchNumber(cli.l1_batch);

    let database_url: SensitiveUrl = cli.database_url.parse().context("invalid --database-url")?;
    let pool = ConnectionPool::<Core>::singleton(database_url)
        .build()
        .await
        .context("failed to build Postgres connection pool")?;

    let object_store_config = ObjectStoreConfig {
        mode: zksync_config::configs::object_store::ObjectStoreMode::FileBacked {
            file_backed_base_path: cli.artifacts_path.clone(),
        },
        ..ObjectStoreConfig::default()
    };
    let blob_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await
        .context("failed to create object store")?;

    let l2_chain_id = L2ChainId::new(cli.l2_chain_id)
        .map_err(|e| anyhow::anyhow!("invalid --l2-chain-id: {e}"))?;

    let (input_a, input_b) = split(
        &pool,
        blob_store.as_ref(),
        SplitParams {
            batch,
            l2_chain_id,
            commitment_chaining: cli.commitment_chaining,
        },
    )
    .await
    .with_context(|| format!("failed to split batch {batch}"))?;

    let key_a = blob_store
        .put((batch, Half::A), &SplitPiece(input_a))
        .await
        .context("failed to write piece A")?;
    let key_b = blob_store
        .put((batch, Half::B), &SplitPiece(input_b))
        .await
        .context("failed to write piece B")?;

    println!("Split batch {batch} into two pieces:");
    println!("  A: {key_a}");
    println!("  B: {key_b}");
    Ok(())
}
