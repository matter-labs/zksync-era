use std::str::FromStr;

use anyhow::Context as _;
use clap::Args as ClapArgs;
use zksync_basic_types::{
    protocol_version::{
        L1VerifierConfig, ProtocolSemanticVersion, ProtocolVersionId, VersionPatch,
    },
    H256,
};
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_prover_dal::{Prover, ProverDal};

use crate::cli::ProverCLIConfig;

#[derive(ClapArgs)]
pub struct Args {
    #[clap(short, long)]
    pub version: u16,
    #[clap(short, long)]
    pub patch: u32,
    #[clap(short, long)]
    pub snark_wrapper: String,
    #[clap(short, long)]
    pub fflonk_snark_wrapper: String,
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

    let snark_wrapper_vk_hash = H256::from_str(&args.snark_wrapper).unwrap_or_else(|_| {
        panic!("Invalid snark wrapper hash");
    });

    let fflonk_snark_wrapper_vk_hash =
        H256::from_str(&args.fflonk_snark_wrapper).unwrap_or_else(|_| {
            panic!("Invalid FFLONK snark wrapper hash");
        });

    conn.fri_protocol_versions_dal()
        .save_prover_protocol_version(
            ProtocolSemanticVersion::new(protocol_version, protocol_version_patch),
            L1VerifierConfig {
                snark_wrapper_vk_hash,
                fflonk_snark_wrapper_vk_hash: Some(fflonk_snark_wrapper_vk_hash),
            },
        )
        .await
        .unwrap();

    Ok(())
}
