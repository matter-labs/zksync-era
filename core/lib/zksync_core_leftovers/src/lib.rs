#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

use std::str::FromStr;

use anyhow::Context as _;
use tokio::sync::oneshot;
use zksync_config::{configs::DatabaseSecrets, GenesisConfig};
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_node_genesis::{ensure_genesis_state, GenesisParams};

pub mod temp_config_store;

/// Inserts the initial information about ZKsync tokens into the database.
pub async fn genesis_init(
    genesis_config: GenesisConfig,
    database_secrets: &DatabaseSecrets,
) -> anyhow::Result<()> {
    let db_url = database_secrets.master_url()?;
    let pool = ConnectionPool::<Core>::singleton(db_url)
        .build()
        .await
        .context("failed to build connection_pool")?;
    let mut storage = pool.connection().await.context("connection()")?;

    let params = GenesisParams::load_genesis_params(genesis_config)?;
    ensure_genesis_state(&mut storage, &params).await?;

    Ok(())
}

pub async fn is_genesis_needed(database_secrets: &DatabaseSecrets) -> bool {
    let db_url = database_secrets.master_url().unwrap();
    let pool = ConnectionPool::<Core>::singleton(db_url)
        .build()
        .await
        .expect("failed to build connection_pool");
    let mut storage = pool.connection().await.expect("connection()");
    storage.blocks_dal().is_genesis_needed().await.unwrap()
}

/// Sets up an interrupt handler and returns a future that resolves once an interrupt signal
/// is received.
pub fn setup_sigint_handler() -> oneshot::Receiver<()> {
    let (sigint_sender, sigint_receiver) = oneshot::channel();
    let mut sigint_sender = Some(sigint_sender);
    ctrlc::set_handler(move || {
        if let Some(sigint_sender) = sigint_sender.take() {
            sigint_sender.send(()).ok();
            // ^ The send fails if `sigint_receiver` is dropped. We're OK with this,
            // since at this point the node should be stopping anyway, or is not interested
            // in listening to interrupt signals.
        }
    })
    .expect("Error setting Ctrl+C handler");

    sigint_receiver
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Component {
    /// Public Web3 API running on HTTP server.
    HttpApi,
    /// Public Web3 API (including PubSub) running on WebSocket server.
    WsApi,
    /// REST API for contract verification.
    ContractVerificationApi,
    /// Metadata calculator.
    Tree,
    /// Merkle tree API.
    TreeApi,
    EthWatcher,
    /// Eth tx generator.
    EthTxAggregator,
    /// Manager for eth tx.
    EthTxManager,
    /// State keeper.
    StateKeeper,
    /// Produces input for the TEE verifier.
    /// The blob is later used as input for TEE verifier.
    TeeVerifierInputProducer,
    /// Component for housekeeping task such as cleaning blobs from GCS, reporting metrics etc.
    Housekeeper,
    /// Component for exposing APIs to prover for providing proof generation data and accepting proofs.
    ProofDataHandler,
    /// Component generating BFT consensus certificates for L2 blocks.
    Consensus,
    /// Component generating commitment for L1 batches.
    CommitmentGenerator,
    /// VM runner-based component that saves protective reads to Postgres.
    VmRunnerProtectiveReads,
}

#[derive(Debug)]
pub struct Components(pub Vec<Component>);

impl FromStr for Components {
    type Err = String;

    fn from_str(s: &str) -> Result<Components, String> {
        match s {
            "api" => Ok(Components(vec![
                Component::HttpApi,
                Component::WsApi,
                Component::ContractVerificationApi,
            ])),
            "http_api" => Ok(Components(vec![Component::HttpApi])),
            "ws_api" => Ok(Components(vec![Component::WsApi])),
            "contract_verification_api" => Ok(Components(vec![Component::ContractVerificationApi])),
            "tree" => Ok(Components(vec![Component::Tree])),
            "tree_api" => Ok(Components(vec![Component::TreeApi])),
            "state_keeper" => Ok(Components(vec![Component::StateKeeper])),
            "housekeeper" => Ok(Components(vec![Component::Housekeeper])),
            "tee_verifier_input_producer" => {
                Ok(Components(vec![Component::TeeVerifierInputProducer]))
            }
            "eth" => Ok(Components(vec![
                Component::EthWatcher,
                Component::EthTxAggregator,
                Component::EthTxManager,
            ])),
            "eth_watcher" => Ok(Components(vec![Component::EthWatcher])),
            "eth_tx_aggregator" => Ok(Components(vec![Component::EthTxAggregator])),
            "eth_tx_manager" => Ok(Components(vec![Component::EthTxManager])),
            "proof_data_handler" => Ok(Components(vec![Component::ProofDataHandler])),
            "consensus" => Ok(Components(vec![Component::Consensus])),
            "commitment_generator" => Ok(Components(vec![Component::CommitmentGenerator])),
            "vm_runner_protective_reads" => {
                Ok(Components(vec![Component::VmRunnerProtectiveReads]))
            }
            other => Err(format!("{} is not a valid component name", other)),
        }
    }
}
