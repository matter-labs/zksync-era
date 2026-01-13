//! Each protocol upgrade required to update genesis config values.
//! This tool generates the new correct genesis file that could be used for the new chain
//! Please note, this tool update only yaml file, if you still use env based configuration,
//! update env values correspondingly

use std::{path::PathBuf, str::FromStr};

use anyhow::Context as _;
use clap::Parser;
use zksync_config::{
    configs::{ContractsGenesis, PostgresSecrets},
    full_config_schema,
    sources::ConfigFilePaths,
};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_node_genesis::{insert_genesis_batch, GenesisParamsInitials};
use zksync_types::url::SensitiveUrl;

pub const DEFAULT_GENESIS_FILE_PATH: &str = "../contracts/configs/genesis/era/latest.json";
#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version, about = "Genesis config generator", long_about = None)]
struct Cli {
    #[arg(long)]
    config_path: Option<std::path::PathBuf>,
    #[arg(long, default_value = "false")]
    check: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();

    let config_file_paths = ConfigFilePaths {
        secrets: opt.config_path,
        ..ConfigFilePaths::default()
    };
    let config_sources =
        tokio::task::spawn_blocking(|| config_file_paths.into_config_sources("ZKSYNC_")).await??;

    let schema = full_config_schema();
    let mut repo = config_sources.build_repository(&schema);
    let database_secrets: PostgresSecrets = repo.parse()?;

    let original_genesis: ContractsGenesis =
        tokio::task::spawn_blocking(|| ContractsGenesis::read(DEFAULT_GENESIS_FILE_PATH.as_ref()))
            .await??;
    let db_url = database_secrets.master_url()?;
    let new_genesis = generate_new_config(db_url, original_genesis.clone()).await?;
    if opt.check {
        assert_eq!(original_genesis, new_genesis);
        println!("Genesis config is up to date");
        return Ok(());
    }
    tokio::task::spawn_blocking(|| new_genesis.write(DEFAULT_GENESIS_FILE_PATH.as_ref())).await??;
    println!("Genesis successfully generated");
    Ok(())
}

async fn generate_new_config(
    db_url: SensitiveUrl,
    genesis_config: ContractsGenesis,
) -> anyhow::Result<ContractsGenesis> {
    let pool = ConnectionPool::<Core>::singleton(db_url)
        .build()
        .await
        .context("failed to build connection_pool")?;

    let mut storage = pool.connection().await.context("connection()")?;
    let mut transaction = storage.start_transaction().await?;

    if !transaction.blocks_dal().is_genesis_needed().await? {
        anyhow::bail!("Please cleanup database for regenerating genesis")
    }

    let base_system_contracts = BaseSystemContracts::load_from_disk().hashes();
    let mut updated_genesis = ContractsGenesis {
        bootloader_hash: base_system_contracts.bootloader,
        default_aa_hash: base_system_contracts.default_aa,
        evm_emulator_hash: base_system_contracts.evm_emulator,
        ..genesis_config
    };

    // This tool doesn't really insert the batch. It doesn't commit the transaction,
    // so the database is clean after using the tool
    let params = GenesisParamsInitials::load_params(&PathBuf::from_str(DEFAULT_GENESIS_FILE_PATH)?);
    let batch_params = insert_genesis_batch(&mut transaction, &params).await?;

    updated_genesis.genesis_batch_commitment = batch_params.commitment;
    updated_genesis.genesis_root = batch_params.root_hash;
    Ok(updated_genesis)
}
