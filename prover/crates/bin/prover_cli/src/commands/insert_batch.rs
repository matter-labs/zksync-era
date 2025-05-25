use anyhow::Context as _;
use chrono::{DateTime, Utc};
use clap::Args as ClapArgs;
use zksync_basic_types::{
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    L1BatchNumber,
};
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_prover_dal::{Prover, ProverDal};
use zksync_types::{L1BatchId, L2ChainId};

use crate::cli::ProverCLIConfig;

#[derive(ClapArgs)]
pub struct Args {
    #[clap(short, long)]
    pub number: L1BatchNumber,
    #[clap(short, long)]
    pub version: u16,
    #[clap(short, long)]
    pub patch: u32,
}

pub async fn run(args: Args, config: ProverCLIConfig) -> anyhow::Result<()> {
    let connection = ConnectionPool::<Prover>::singleton(config.db_url)
        .build()
        .await
        .context("failed to build a prover_connection_pool")?;
    let mut conn = connection.connection().await.unwrap();

    let protocol_version = ProtocolVersionId::try_from(args.version)
        .map_err(|_| anyhow::anyhow!("Invalid protocol version"))?;

    let protocol_version_patch = VersionPatch(args.patch);

    conn.fri_basic_witness_generator_dal()
        .save_witness_inputs(
            L1BatchId::new(L2ChainId::zero(), args.number),
            &format!("witness_inputs_{}", args.number.0),
            ProtocolSemanticVersion::new(protocol_version, protocol_version_patch),
            DateTime::<Utc>::default(),
        )
        .await
        .unwrap();

    Ok(())
}
