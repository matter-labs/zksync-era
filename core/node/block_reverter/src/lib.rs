use std::{path::Path, time::Duration};

use anyhow::Context as _;
use bitflags::bitflags;
use serde::Serialize;
use tokio::fs;
use zksync_config::{ContractsConfig, EthConfig};
use zksync_contracts::zksync_contract;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_signer::{EthereumSigner, PrivateKeySigner, TransactionParameters};
use zksync_merkle_tree::domain::ZkSyncTree;
use zksync_state::RocksdbStorage;
use zksync_storage::RocksDB;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    ethabi::Token,
    web3::{
        contract::{Contract, Options},
        transports::Http,
        types::{BlockId, BlockNumber},
        Web3,
    },
    Address, L1BatchNumber, H160, H256, U256,
};

bitflags! {
    pub struct BlockReverterFlags: u32 {
        const POSTGRES = 0b_0001;
        const TREE = 0b_0010;
        const SK_CACHE = 0b_0100;
    }
}

#[derive(Debug)]
pub struct BlockReverterEthConfig {
    eth_client_url: String,
    reverter_private_key: Option<H256>,
    reverter_address: Option<Address>,
    diamond_proxy_addr: H160,
    validator_timelock_addr: H160,
    default_priority_fee_per_gas: u64,
}

impl BlockReverterEthConfig {
    pub fn new(
        eth_config: EthConfig,
        contract: ContractsConfig,
        reverter_address: Option<Address>,
    ) -> anyhow::Result<Self> {
        #[allow(deprecated)]
        // `BlockReverter` doesn't support non env configs yet
        let pk = eth_config
            .sender
            .context("eth_sender_config")?
            .private_key();

        Ok(Self {
            eth_client_url: eth_config.web3_url,
            reverter_private_key: pk,
            reverter_address,
            diamond_proxy_addr: contract.diamond_proxy_addr,
            validator_timelock_addr: contract.validator_timelock_addr,
            default_priority_fee_per_gas: eth_config
                .gas_adjuster
                .context("gas adjuster")?
                .default_priority_fee_per_gas,
        })
    }
}

/// Role of the node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeRole {
    Main,
    External,
}

/// This struct is used to perform a rollback of the state.
/// Rollback is a rare event of manual intervention, when the node operator
/// decides to revert some of the not yet finalized batches for some reason
/// (e.g. inability to generate a proof).
///
/// It is also used to automatically perform a rollback on the external node
/// after it is detected on the main node.
///
/// There are a few state components that we can roll back:
///
/// - State of the Postgres database
/// - State of the Merkle tree
/// - State of the state keeper cache
/// - State of the Ethereum contract (if the block was committed)
#[derive(Debug)]
pub struct BlockReverter {
    /// It affects the interactions with the consensus state.
    /// This distinction will be removed once consensus genesis is moved to the L1 state.
    node_role: NodeRole,
    state_keeper_cache_path: String,
    merkle_tree_path: String,
    connection_pool: ConnectionPool<Core>,
    allow_reverting_executed_batches: bool,
}

impl BlockReverter {
    pub fn new(
        node_role: NodeRole,
        state_keeper_cache_path: String,
        merkle_tree_path: String,
        connection_pool: ConnectionPool<Core>,
    ) -> Self {
        Self {
            node_role,
            state_keeper_cache_path,
            merkle_tree_path,
            connection_pool,
            allow_reverting_executed_batches: false,
        }
    }

    /// Allows reverting the state past the last batch finalized on L1. If this is disallowed (which is the default),
    /// block reverter will error upon such an attempt.
    ///
    /// Main use case for the setting this flag is the external node, where may obtain an
    /// incorrect state even for a block that was marked as executed. On the EN, this mode is not destructive.
    pub fn allow_reverting_executed_batches(&mut self) {
        self.allow_reverting_executed_batches = true;
    }

    /// Rolls back DBs (Postgres + RocksDB) to a previous state.
    pub async fn rollback_db(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
        flags: BlockReverterFlags,
    ) -> anyhow::Result<()> {
        let rollback_tree = flags.contains(BlockReverterFlags::TREE);
        let rollback_postgres = flags.contains(BlockReverterFlags::POSTGRES);
        let rollback_sk_cache = flags.contains(BlockReverterFlags::SK_CACHE);

        if !self.allow_reverting_executed_batches {
            let mut storage = self.connection_pool.connection().await?;
            let last_executed_l1_batch = storage
                .blocks_dal()
                .get_number_of_last_l1_batch_executed_on_eth()
                .await?;
            anyhow::ensure!(
                Some(last_l1_batch_to_keep) >= last_executed_l1_batch,
                "Attempt to revert already executed L1 batches; the last executed batch is: {last_executed_l1_batch:?}"
            );
        }

        // Tree needs to be reverted first to keep state recoverable
        self.rollback_rocks_dbs(last_l1_batch_to_keep, rollback_tree, rollback_sk_cache)
            .await?;
        if rollback_postgres {
            self.rollback_postgres(last_l1_batch_to_keep).await?;
        }
        Ok(())
    }

    async fn rollback_rocks_dbs(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
        rollback_tree: bool,
        rollback_sk_cache: bool,
    ) -> anyhow::Result<()> {
        if rollback_tree {
            let storage_root_hash = self
                .connection_pool
                .connection()
                .await?
                .blocks_dal()
                .get_l1_batch_state_root(last_l1_batch_to_keep)
                .await?
                .context("failed to fetch root hash for target L1 batch")?;

            // Rolling back Merkle tree
            let merkle_tree_path = Path::new(&self.merkle_tree_path);
            let merkle_tree_exists = fs::try_exists(merkle_tree_path).await.with_context(|| {
                format!(
                    "cannot check whether Merkle tree path `{}` exists",
                    merkle_tree_path.display()
                )
            })?;
            if merkle_tree_exists {
                tracing::info!("Reverting Merkle tree at {}", merkle_tree_path.display());
                let merkle_tree_path = merkle_tree_path.to_path_buf();
                tokio::task::spawn_blocking(move || {
                    Self::revert_tree_blocking(
                        last_l1_batch_to_keep,
                        &merkle_tree_path,
                        storage_root_hash,
                    )
                })
                .await
                .context("reverting Merkle tree panicked")??;
            } else {
                tracing::info!(
                    "Merkle tree not found at `{}`; skipping",
                    merkle_tree_path.display()
                );
            }
        }

        if rollback_sk_cache {
            let sk_cache_exists = fs::try_exists(&self.state_keeper_cache_path)
                .await
                .with_context(|| {
                    format!(
                        "cannot check whether state keeper cache path `{}` exists",
                        self.state_keeper_cache_path
                    )
                })?;
            anyhow::ensure!(
                sk_cache_exists,
                "Path with state keeper cache DB doesn't exist at {}",
                self.state_keeper_cache_path
            );
            self.rollback_state_keeper_cache(last_l1_batch_to_keep)
                .await?;
        }
        Ok(())
    }

    fn revert_tree_blocking(
        last_l1_batch_to_keep: L1BatchNumber,
        path: &Path,
        storage_root_hash: H256,
    ) -> anyhow::Result<()> {
        let db = RocksDB::new(path).context("failed initializing RocksDB for Merkle tree")?;
        let mut tree = ZkSyncTree::new_lightweight(db.into());

        if tree.next_l1_batch_number() <= last_l1_batch_to_keep {
            tracing::info!("Tree is behind the L1 batch to revert to; skipping");
            return Ok(());
        }
        tree.revert_logs(last_l1_batch_to_keep);

        tracing::info!("Checking match of the tree root hash and root hash from Postgres");
        let tree_root_hash = tree.root_hash();
        anyhow::ensure!(
            tree_root_hash == storage_root_hash,
            "Mismatch between the tree root hash {tree_root_hash:?} and storage root hash {storage_root_hash:?} after revert"
        );
        tracing::info!("Saving tree changes to disk");
        tree.save();
        Ok(())
    }

    /// Reverts blocks in the state keeper cache.
    async fn rollback_state_keeper_cache(
        &self,
        last_l1_batch_to_keep: L1BatchNumber,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Opening DB with state keeper cache at `{}`",
            self.state_keeper_cache_path
        );
        let sk_cache = RocksdbStorage::builder(self.state_keeper_cache_path.as_ref())
            .await
            .context("failed initializing state keeper cache")?;

        if sk_cache.l1_batch_number().await > Some(last_l1_batch_to_keep + 1) {
            let mut storage = self.connection_pool.connection().await?;
            tracing::info!("Rolling back state keeper cache...");
            sk_cache
                .rollback(&mut storage, last_l1_batch_to_keep)
                .await
                .context("failed rolling back state keeper cache")?;
        } else {
            tracing::info!("Nothing to revert in state keeper cache");
        }
        Ok(())
    }

    /// Reverts data in the Postgres database.
    /// If `node_role` is `Main` a consensus hard-fork is performed.
    async fn rollback_postgres(&self, last_l1_batch_to_keep: L1BatchNumber) -> anyhow::Result<()> {
        tracing::info!("Rolling back postgres data");
        let mut storage = self.connection_pool.connection().await?;
        let mut transaction = storage.start_transaction().await?;

        let (_, last_l2_block_to_keep) = transaction
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(last_l1_batch_to_keep)
            .await?
            .with_context(|| {
                format!("L1 batch #{last_l1_batch_to_keep} doesn't contain L2 blocks")
            })?;

        tracing::info!("Rolling back transactions state");
        transaction
            .transactions_dal()
            .reset_transactions_state(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back events");
        transaction
            .events_dal()
            .rollback_events(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back L2 to L1 logs");
        transaction
            .events_dal()
            .rollback_l2_to_l1_logs(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back created tokens");
        transaction
            .tokens_dal()
            .rollback_tokens(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back factory deps");
        transaction
            .factory_deps_dal()
            .rollback_factory_deps(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back storage");
        #[allow(deprecated)]
        transaction
            .storage_logs_dal()
            .rollback_storage(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back storage logs");
        transaction
            .storage_logs_dal()
            .rollback_storage_logs(last_l2_block_to_keep)
            .await?;
        tracing::info!("Rolling back Ethereum transactions");
        transaction
            .eth_sender_dal()
            .delete_eth_txs(last_l1_batch_to_keep)
            .await?;
        tracing::info!("Rolling back L1 batches");
        transaction
            .blocks_dal()
            .delete_l1_batches(last_l1_batch_to_keep)
            .await?;
        transaction
            .blocks_dal()
            .delete_initial_writes(last_l1_batch_to_keep)
            .await?;
        tracing::info!("Rolling back L2 blocks...");
        transaction
            .blocks_dal()
            .delete_l2_blocks(last_l2_block_to_keep)
            .await?;

        if self.node_role == NodeRole::Main {
            tracing::info!("Performing consensus hard fork");
            transaction.consensus_dal().fork().await?;
        }

        transaction.commit().await?;
        Ok(())
    }

    /// Sends a revert transaction to L1.
    pub async fn send_ethereum_revert_transaction(
        &self,
        eth_config: &BlockReverterEthConfig,
        last_l1_batch_to_keep: L1BatchNumber,
        priority_fee_per_gas: U256,
        nonce: u64,
    ) -> anyhow::Result<()> {
        let web3 =
            Web3::new(Http::new(&eth_config.eth_client_url).context("cannot create L1 client")?);
        let contract = zksync_contract();
        let signer = PrivateKeySigner::new(
            eth_config
                .reverter_private_key
                .context("private key is required to send revert transaction")?,
        );
        let chain_id = web3
            .eth()
            .chain_id()
            .await
            .context("failed getting L1 chain ID")?
            .as_u64();

        let revert_function = contract
            .function("revertBlocks")
            .or_else(|_| contract.function("revertBatches"))
            .context(
                "Either `revertBlocks` or `revertBatches` function must be present in contract",
            )?;
        let data = revert_function
            .encode_input(&[Token::Uint(last_l1_batch_to_keep.0.into())])
            .context("failed encoding revert function input")?;

        let base_fee = web3
            .eth()
            .block(BlockId::Number(BlockNumber::Pending))
            .await
            .context("failed getting pending L1 block")?
            .map(|block| {
                block
                    .base_fee_per_gas
                    .context("no base_fee_per_gas in pending block")
            })
            .transpose()?;
        let base_fee = if let Some(base_fee) = base_fee {
            base_fee
        } else {
            // Pending block doesn't exist, use the latest one.
            web3.eth()
                .block(BlockId::Number(BlockNumber::Latest))
                .await
                .context("failed geting latest L1 block")?
                .context("no latest L1 block")?
                .base_fee_per_gas
                .context("no base_fee_per_gas in latest block")?
        };

        let tx = TransactionParameters {
            to: eth_config.validator_timelock_addr.into(),
            data,
            chain_id,
            nonce: nonce.into(),
            max_priority_fee_per_gas: priority_fee_per_gas,
            max_fee_per_gas: base_fee + priority_fee_per_gas,
            gas: 5_000_000.into(),
            ..Default::default()
        };

        let signed_tx = signer
            .sign_transaction(tx)
            .await
            .context("cannot sign revert transaction")?;
        let hash = web3
            .eth()
            .send_raw_transaction(signed_tx.into())
            .await
            .context("failed sending revert transaction")?;
        tracing::info!("Sent revert transaction to L1 with hash {hash:?}");

        loop {
            let maybe_receipt = web3
                .eth()
                .transaction_receipt(hash)
                .await
                .context("failed getting receipt for revert transaction")?;
            if let Some(receipt) = maybe_receipt {
                anyhow::ensure!(
                    receipt.status == Some(1.into()),
                    "Revert transaction {hash:?} failed with status {:?}",
                    receipt.status
                );
                tracing::info!("Revert transaction has completed");
                return Ok(());
            } else {
                tracing::info!("waiting for L1 transaction confirmation...");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }

    #[tracing::instrument(skip(contract), err, fields(contract.address = ?contract.address()))]
    async fn get_l1_batch_number_from_contract(
        contract: &Contract<Http>,
        op: AggregatedActionType,
    ) -> anyhow::Result<L1BatchNumber> {
        let function_name = match op {
            AggregatedActionType::Commit => "getTotalBatchesCommitted",
            AggregatedActionType::PublishProofOnchain => "getTotalBatchesVerified",
            AggregatedActionType::Execute => "getTotalBatchesExecuted",
        };
        let block_number: U256 = contract
            .query(function_name, (), None, Options::default(), None)
            .await
            .with_context(|| {
                format!(
                    "failed calling `{function_name}` for contract {:?}",
                    contract.address()
                )
            })?;
        Ok(L1BatchNumber(block_number.as_u32()))
    }

    /// Returns suggested values for rollback.
    pub async fn suggested_values(
        &self,
        eth_config: &BlockReverterEthConfig,
    ) -> anyhow::Result<SuggestedRollbackValues> {
        let web3 =
            Web3::new(Http::new(&eth_config.eth_client_url).context("cannot create L1 client")?);
        let contract_address = eth_config.diamond_proxy_addr;
        let contract = Contract::new(web3.eth(), contract_address, zksync_contract());

        let last_committed_l1_batch_number =
            Self::get_l1_batch_number_from_contract(&contract, AggregatedActionType::Commit)
                .await?;
        let last_verified_l1_batch_number = Self::get_l1_batch_number_from_contract(
            &contract,
            AggregatedActionType::PublishProofOnchain,
        )
        .await?;
        let last_executed_l1_batch_number =
            Self::get_l1_batch_number_from_contract(&contract, AggregatedActionType::Execute)
                .await?;

        tracing::info!(
            "Last L1 batch numbers on contract: committed {last_committed_l1_batch_number}, \
             verified {last_verified_l1_batch_number}, executed {last_executed_l1_batch_number}"
        );

        let priority_fee = eth_config.default_priority_fee_per_gas;
        let reverter_address = eth_config
            .reverter_address
            .context("need to provide operator address to suggest reversion values")?;
        let nonce = web3
            .eth()
            .transaction_count(reverter_address, Some(BlockNumber::Pending))
            .await
            .with_context(|| format!("failed getting transaction count for {reverter_address:?}"))?
            .as_u64();

        Ok(SuggestedRollbackValues {
            last_executed_l1_batch_number,
            nonce,
            priority_fee,
        })
    }

    /// Clears failed L1 transactions.
    pub async fn clear_failed_l1_transactions(&self) -> anyhow::Result<()> {
        tracing::info!("Clearing failed L1 transactions");
        self.connection_pool
            .connection()
            .await?
            .eth_sender_dal()
            .clear_failed_transactions()
            .await?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct SuggestedRollbackValues {
    pub last_executed_l1_batch_number: L1BatchNumber,
    pub nonce: u64,
    pub priority_fee: u64,
}
