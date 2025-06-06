use std::alloc::Global;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Context;
use ruint::aliases::U256;
use tokio::sync::watch;
use zk_os_basic_system::system_implementation::flat_storage_model::TestingTree;
use zk_os_basic_system::system_implementation::system::BatchOutput;
use zk_os_forward_system::run::{BatchContext, StorageCommitment};
use zk_os_forward_system::run::test_impl::{InMemoryPreimageSource, InMemoryTree, NoopTxCallback, TxListSource};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_l1_contract_interface::i_executor::{batch_output_hash_as_register_values, batch_public_input};
use zksync_l1_contract_interface::zkos_commitment_to_vm_batch_output;
use zksync_types::{H256, L1BatchNumber, L2BlockNumber};
use zksync_types::block::L2BlockHeader;
use zksync_types::commitment::ZkosCommitment;
use zksync_zkos_vm_runner::zkos_conversions::{h256_to_bytes32, tx_abi_encode};
use crate::zkos_proof_data_server::run;

pub struct ZkosProverInputGenerator {
    stop_receiver: watch::Receiver<bool>,
    pool: ConnectionPool<Core>,
}

impl ZkosProverInputGenerator {
    pub fn new(
        stop_receiver: watch::Receiver<bool>,
        pool: ConnectionPool<Core>,
    ) -> ZkosProverInputGenerator {
        Self {
            stop_receiver,
            pool,
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let mut connection =
            self.pool.connection_tagged("zkos_prover_input_generator").await?;


        // get from env vars
        let dry_run_block_number = std::env::var("DRY_RUN_BLOCK")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .map(L2BlockNumber);

        if let Some(dry_run_block) = dry_run_block_number {
            tracing::info!("Starting Prover Input Generator - initializing in-memory storages for block {:?}", dry_run_block - 1);
            let (mut tree, mut preimages) = self.initialize_in_memory_storages(dry_run_block - 1).await?;
            tracing::info!("initialized in-memory storages");

            self.dry_run_block(&mut tree, &mut preimages, dry_run_block).await?;
            tracing::info!("Dry run completed - exiting");
            return Ok(());
        }

        // todo: should be a different layer
        tracing::info!("Starting HTTP server in a separate thread");
        tokio::spawn(run(self.pool.clone()));

        let last_processed_block = connection.zkos_prover_dal().last_block_with_generated_input().await?;


        tracing::info!("Starting Prover Input Generator - initializing in-memory storages - last processed block {:?}", last_processed_block);
        let (mut tree, mut preimages) = self.initialize_in_memory_storages(last_processed_block).await?;
        tracing::info!("initialized in-memory storages");

        let mut next_block_to_process = last_processed_block + 1;

        loop {
            if *self.stop_receiver.borrow() {
                tracing::info!("Prover Input Generator was interrupted");
                return Ok(());
            }

            let mut connection =
                self.pool.connection_tagged("zkos_prover_input_generator").await?;


            if let Some(block) = connection.blocks_dal().get_l2_block_header(next_block_to_process).await? {
                tracing::info!("Processing block {:?}", block);
                let started_at = std::time::Instant::now();

                let prover_input =
                    self.generate_prover_input_for_block(tree.clone(), preimages.clone(), &block).await?;

                let duration = started_at.elapsed();

                tracing::info!(
                    "Prover Input Generator finished processing block {:?}; took {:?} seconds",
                    block.number.0,
                    duration
                );

                Self::print_expected_values_for_block(&mut connection, &block).await?;

                connection.zkos_prover_dal().insert_prover_input(
                    block.number,
                    prover_input,
                    duration,
                ).await?;

                // apply changes to in-memory tree/storage
                // will be removed when the persistent tree is used

                self.apply_block_diff_to_in_memory_storages(
                    block.number,
                    &mut tree,
                    &mut preimages,
                ).await?;

                next_block_to_process += 1;
            } else {
                tracing::trace!("No blocks to process - waiting");

                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        }
    }

    async fn dry_run_block(
        self,
        mut tree: &mut InMemoryTree,
        mut preimages: &mut InMemoryPreimageSource,
        dry_run_block: L2BlockNumber
    ) -> anyhow::Result<()> {
        let mut connection =
            self.pool.connection_tagged("zkos_prover_input_generator").await?;

        let block= connection
            .blocks_dal()
            .get_l2_block_header(dry_run_block)
            .await?
            .context("Block not found")?;


        tracing::info!("Dry run mode enabled - processing block {:?}", dry_run_block);
        let prover_input =
            self.generate_prover_input_for_block(tree.clone(), preimages.clone(), &block).await?;

        tracing::info!("Dry run completed - generated prover input for block {:?}", dry_run_block);

        Self::print_expected_values_for_block(&mut connection, &block).await?;
        Ok(())
    }

    async fn print_expected_values_for_block(connection: &mut Connection<'_, Core>, block: &L2BlockHeader) -> anyhow::Result<()> {
        let batch = connection
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(block.number.0))
            .await?
            .context("Failed to get L1 batch header for the block")?;

        let prev_batch = connection
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(block.number.0 - 1))
            .await?
            .context("Failed to get L1 batch header for the block")?;

        let expected_batch_public_input = batch_public_input(&prev_batch, &batch);
        tracing::info!("Block {} Expected batch public input: {:?}", block.number.0, expected_batch_public_input);
        tracing::info!("Block {} Expected batch public input hash: {:?}", block.number.0, H256::from_slice(&expected_batch_public_input.hash()));
        tracing::info!("Block {} Expected batch public input hash registers: {:?}", block.number.0, batch_output_hash_as_register_values(&expected_batch_public_input));
        tracing::info!("Block {} Expected batch output preimage: {:?}", block.number.0, zkos_commitment_to_vm_batch_output(&ZkosCommitment::from(&batch)));
        tracing::info!("Block {} full pubdata: {:?}", block.number.0, batch.header.pubdata_input);
        Ok(())
    }

    async fn generate_prover_input_for_block(
        &self,
        tree: InMemoryTree,
        preimages: InMemoryPreimageSource,
        block: &L2BlockHeader
    ) -> anyhow::Result<Vec<u32>> {
        let context = BatchContext {
            //todo: gas
            eip1559_basefee: U256::from(block.base_fee_per_gas),
            // copied from keeper.rs
            native_price: U256::from(block.base_fee_per_gas / 100),
            gas_per_pubdata: Default::default(),
            block_number: block.number.0 as u64,
            timestamp: block.timestamp,
            // todo: get from config
            chain_id: 271,
            // TODO: copied from `keeper.rs`
            gas_limit: 100_000_000,
            coinbase: Default::default(),
            block_hashes: Default::default(),
        };

        let storage_commitment = StorageCommitment {
            root: tree.storage_tree.root().clone(),
            next_free_slot: tree.storage_tree.next_free_slot,
        };

        let transactions = self.load_transactions_for_block(block.number).await?;
        tracing::info!("Prover Input Generator is processing block {:?} with  {:?} transactions. Context: {:?}", block.number.0, transactions.len(), context);

        let list_source = TxListSource { transactions };
        let prover_input = zk_os_forward_system::run::generate_proof_input(
            PathBuf::from("app_logging_enabled.bin"),
            context,
            storage_commitment,
            tree,
            preimages,
            list_source,
        ).map_err(|err| anyhow::anyhow!("{}", err.0).context("zk_ee internal error"))?;

        Ok(prover_input)
    }

    async fn load_transactions_for_block(&self, block_number: L2BlockNumber) -> anyhow::Result<VecDeque<Vec<u8>>> {
        let mut connection =
            self.pool.connection_tagged("zkos_prover_input_generator").await?;


        // todo: relying on miniblock === l1 batch
        let l2_blocks = connection
            .transactions_dal()
            .get_l2_blocks_to_execute_for_l1_batch(L1BatchNumber(block_number.0))
            .await?;

        assert_eq!(l2_blocks.len(), 1, "Expected exactly one miniblock for the given l1 batch");

        let l2_block = l2_blocks.into_iter().next().unwrap();
        let transactions = l2_block.txs
            .into_iter()
            .map(|tx| tx_abi_encode(tx))
            .collect::<VecDeque<_>>();
        Ok(transactions)
    }

    async fn apply_block_diff_to_in_memory_storages(
        &self,
        block_number: L2BlockNumber,
        tree: &mut InMemoryTree,
        preimage_source: &mut InMemoryPreimageSource,
    ) -> anyhow::Result<()>{
        let mut conn = self.pool.connection_tagged("zkos_prover_input_generator").await?;

        let storage_logs = conn
            .storage_logs_dal()
            .storage_logs_for_block(block_number)
            .await;

        let mut preimages: HashMap<H256, Vec<u8>> = HashMap::new();

        let factory_deps: HashMap<H256, Vec<u8>> = conn
            .factory_deps_dal()
            .get_factory_deps_for_block(block_number)
            .await;

        let account_props: HashMap<H256, Vec<u8>> = conn
            .account_properies_dal()
            .get_l1_batch_account_properties(L1BatchNumber(block_number.0))
            .await?;

        tracing::info!(
            "Applying block diff for block {:?} with {:?} storage logs, {:?} factory_deps and {:?} account properties",
            block_number,
            storage_logs.len(),
            factory_deps.len(),
            account_props.len()
        );

        preimages.extend(factory_deps);
        preimages.extend(account_props);


        for storage_logs in storage_logs {
            // todo: awkwardly we need to insert both into cold_storage and storage_tree
            tree.cold_storage.insert(
                h256_to_bytes32(storage_logs.hashed_key),
                h256_to_bytes32(storage_logs.value),
            );
            tree.storage_tree.insert(
                &h256_to_bytes32(storage_logs.hashed_key),
                &h256_to_bytes32(storage_logs.value),
            );
        }


        for (hash, value) in preimages {
            preimage_source
                .inner
                .insert(h256_to_bytes32(hash), value);
        }
        Ok(())
    }

    async fn initialize_in_memory_storages(&self, l2_block_number: L2BlockNumber) -> anyhow::Result<(InMemoryTree, InMemoryPreimageSource)> {
        let mut tree = InMemoryTree {
            storage_tree: TestingTree::new_in(Global),
            cold_storage: HashMap::new(),
        };
        let mut preimage_source = InMemoryPreimageSource {
            inner: Default::default(),
        };


        let mut conn = self.pool.connection_tagged("zkos_prover_input_generator").await?;

        let all_storage_logs = conn
            .storage_logs_dal()
            .dump_all_storage_logs_until_batch(l2_block_number + 1)
            .await;

        let mut preimages = conn
            .factory_deps_dal()
            .dump_all_factory_deps_for_tests()
            .await;

        // iterate from 1 to l2_block_number
        for i in 1..=l2_block_number.0 {
            let account_props = conn
                .account_properies_dal()
                .get_l1_batch_account_properties(L1BatchNumber(i))
                .await?;
            for (hash, value) in account_props {
                preimages.insert(hash, value);
            }
        }

        tracing::info!(
            "Loaded from DB: {:?} storage logs and {:?} preimages",
            all_storage_logs.len(),
            preimages.len()
        );
        tracing::info!("Recovering tree from storage logs...");

        for storage_logs in all_storage_logs {
            // todo: awkwardly we need to insert both into cold_storage and storage_tree
            tree.cold_storage.insert(
                h256_to_bytes32(storage_logs.hashed_key),
                h256_to_bytes32(storage_logs.value),
            );
            tree.storage_tree.insert(
                &h256_to_bytes32(storage_logs.hashed_key),
                &h256_to_bytes32(storage_logs.value),
            );
        }
        tracing::info!("Tree recovery complete");

        tracing::info!("Recovering preimages...");

        for (hash, value) in preimages {
            preimage_source
                .inner
                .insert(h256_to_bytes32(hash), value);
        }

        tracing::info!("Preimage recovery complete");
        Ok((tree, preimage_source))
    }
}