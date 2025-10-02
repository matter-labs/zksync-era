use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use zksync_core_leftovers::temp_config_store::read_yaml_repr;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_l1_recovery::{BlobKey, BlobWrapper};
use zksync_object_store::ObjectStoreFactory;
use zksync_types::eth_sender::EthTxBlobSidecar;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about)]
struct Cli {
    #[arg(long, global = true)]
    secrets_path: PathBuf,

    #[arg(long, global = true)]
    config_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Cli::parse();
    let secrets_config =
        read_yaml_repr::<zksync_protobuf_config::proto::secrets::Secrets>(&opts.secrets_path)
            .context("failed decoding secrets YAML config")?;
    let database_secrets = secrets_config
        .database
        .clone()
        .context("Failed to find database config")?;

    let connection_pool = ConnectionPool::<Core>::singleton(database_secrets.master_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;

    let general_config =
        read_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&opts.config_path)
            .context("failed decoding general YAML config")?;
    let object_store_config = general_config
        .snapshot_recovery
        .unwrap()
        .object_store
        .context("failed to find core object store config")?;
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await?;

    let mut id = 1;
    loop {
        let mut storage = connection_pool.connection().await.unwrap();
        let tx = storage.eth_sender_dal().get_eth_tx(id).await.unwrap();
        id += 1;
        if tx.is_none() {
            break;
        }

        if let Some(blob_sidecar) = tx.unwrap().blob_sidecar {
            match blob_sidecar {
                EthTxBlobSidecar::EthTxBlobSidecarV1(sidecar) => {
                    for blob in sidecar.blobs {
                        object_store
                            .put(
                                BlobKey {
                                    kzg_commitment: blob
                                        .commitment
                                        .try_into()
                                        .expect("unable to convert kzg_commitment to [u8; 48]"),
                                },
                                &BlobWrapper { blob: blob.blob },
                            )
                            .await?;
                    }
                }
            }
        }
    }

    println!("Finished dumping blobs");
    Ok(())
}
