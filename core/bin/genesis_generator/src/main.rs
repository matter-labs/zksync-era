/// Each protocol upgrade required to update genesis config values.
/// This tool generates the new correct genesis file that could be used for the new chain
/// Please note, this tool update only yaml file, if you still use env based configuration,
/// update env values correspondingly
use std::fs;

use anyhow::Context as _;
use clap::Parser;
use serde_yaml::Serializer;
use zksync_config::{configs::DatabaseSecrets, GenesisConfig};
use zksync_contracts::BaseSystemContracts;
use zksync_core_leftovers::temp_config_store::decode_yaml_repr;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_env_config::FromEnv;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_protobuf::{
    build::{prost_reflect, prost_reflect::ReflectMessage},
    ProtoRepr,
};
use zksync_protobuf_config::proto::genesis::Genesis;
use zksync_types::{
    protocol_version::ProtocolSemanticVersion, url::SensitiveUrl, ProtocolVersionId,
};

const DEFAULT_GENESIS_FILE_PATH: &str = "./etc/env/file_based/genesis.yaml";

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

    let database_secrets = match opt.config_path {
        None => DatabaseSecrets::from_env()?,
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            let config = decode_yaml_repr::<zksync_protobuf_config::proto::secrets::Secrets>(&yaml)
                .context("failed decoding general YAML config")?;
            config.database.context("Database secrets must exist")?
        }
    };

    let yaml = std::fs::read_to_string(DEFAULT_GENESIS_FILE_PATH)
        .with_context(|| DEFAULT_GENESIS_FILE_PATH.to_string())?;
    let original_genesis = decode_yaml_repr::<Genesis>(&yaml)?;
    let db_url = database_secrets.master_url()?;
    let new_genesis = generate_new_config(db_url, original_genesis.clone()).await?;
    if opt.check {
        assert_eq!(&original_genesis, &new_genesis);
        println!("Genesis config is up to date");
        return Ok(());
    }
    let data = encode_yaml(&Genesis::build(&new_genesis))?;
    fs::write(DEFAULT_GENESIS_FILE_PATH, data)?;
    println!("Genesis successfully generated");
    Ok(())
}

async fn generate_new_config(
    db_url: SensitiveUrl,
    genesis_config: GenesisConfig,
) -> anyhow::Result<GenesisConfig> {
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
    let mut updated_genesis = GenesisConfig {
        protocol_version: Some(ProtocolSemanticVersion {
            minor: ProtocolVersionId::latest(),
            patch: 0.into(), // genesis generator proposes some new valid config, so patch 0 works here.
        }),
        genesis_root_hash: None,
        rollup_last_leaf_index: None,
        genesis_commitment: None,
        bootloader_hash: Some(base_system_contracts.bootloader),
        default_aa_hash: Some(base_system_contracts.default_aa),
        ..genesis_config
    };

    // This tool doesn't really insert the batch. It doesn't commit the transaction,
    // so the database is clean after using the tool
    let params = GenesisParams::load_genesis_params(updated_genesis.clone())?;
    let batch_params = insert_genesis_batch(&mut transaction, &params).await?;

    updated_genesis.genesis_commitment = Some(batch_params.commitment);
    updated_genesis.genesis_root_hash = Some(batch_params.root_hash);
    updated_genesis.rollup_last_leaf_index = Some(batch_params.rollup_last_leaf_index);

    Ok(updated_genesis)
}

/// Encodes a generated proto message to json for arbitrary `ProtoFmt`.
pub(crate) fn encode_yaml<T: ReflectMessage>(x: &T) -> anyhow::Result<String> {
    let mut serializer = Serializer::new(vec![]);
    let opts = prost_reflect::SerializeOptions::new()
        .use_proto_field_name(true)
        .stringify_64_bit_integers(false);
    x.transcode_to_dynamic()
        .serialize_with_options(&mut serializer, &opts)?;
    Ok(String::from_utf8_lossy(&serializer.into_inner()?).to_string())
}
