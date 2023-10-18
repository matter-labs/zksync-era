use std::{
    cmp::Ordering::{Equal, Greater, Less},
    collections::HashMap,
    fmt::Debug,
    time::Duration,
};

use serde_json::Value;
use tokio::{sync::watch::Receiver, time::sleep};

use zksync_types::{
    api::{BlockDetails, BlockNumber, L1BatchDetails},
    web3::types::U64,
    L1BatchNumber, MiniblockNumber, H256,
};
use zksync_utils::wait_for_tasks::wait_for_tasks;
use zksync_web3_decl::{
    jsonrpsee::core::Error,
    jsonrpsee::http_client::{HttpClient, HttpClientBuilder},
    namespaces::{EnNamespaceClient, EthNamespaceClient, ZksNamespaceClient},
    types::FilterBuilder,
    RpcResult,
};

use crate::config::{CheckerConfig, RpcMode};
use crate::{
    divergence::{Divergence, DivergenceDetails},
    helpers::compare_json,
};

#[derive(Debug, Clone)]
pub struct Checker {
    /// 'Triggered' to run once. 'Continuous' to run forever.
    mode: RpcMode,
    /// Client for interacting with the main node.
    main_node_client: HttpClient,
    /// Client for interacting with the instance nodes.
    instance_clients: Vec<InstanceHttpClient>,
    /// Check all miniblocks starting from this. If 'None' then check from genesis. Inclusive.
    start_miniblock: Option<MiniblockNumber>,
    /// For Triggered mode. If 'None' then check all available miniblocks. Inclusive.
    finish_miniblock: Option<MiniblockNumber>,
    /// In seconds, how often to poll the instance node for new miniblocks.
    instance_poll_period: u64,
    /// Maps instance URL to a list of its divergences.
    divergences: HashMap<String, Vec<Divergence>>,
    /// How often should blocks logs be checked.
    log_check_interval: u32,
    /// Next batch number to check for each instance.
    next_batch_to_check: HashMap<String, L1BatchNumber>,
    /// The maximum number of transactions to be checked at random in each miniblock.
    max_transactions_to_check: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct InstanceHttpClient {
    pub url: String,
    pub client: HttpClient,
}

impl Checker {
    pub fn new(config: &CheckerConfig) -> Self {
        let (main_node_client, instance_clients) = Self::setup_clients(
            config
                .main_node_http_url
                .clone()
                .expect("An RPC URL for the main node has to be provided for RPC mode."),
            config
                .instances_http_urls
                .clone()
                .expect("RPC URLs for the EN instances have to be provided for RPC mode."),
        );

        let last_checked_batch = instance_clients
            .iter()
            .map(|instance| (instance.url.clone(), L1BatchNumber(0)))
            .collect();

        let mode = config
            .rpc_mode
            .expect("The RPC Checker has to be provided an RPC mode");

        Self {
            mode,
            main_node_client,
            instance_clients,
            start_miniblock: config.start_miniblock.map(|n| n.into()),
            finish_miniblock: config.finish_miniblock.map(|n| n.into()),
            instance_poll_period: config.instance_poll_period.unwrap_or(10),
            divergences: HashMap::new(),
            log_check_interval: 1, // TODO (BFT-192): make configurable if we want to keep it.
            next_batch_to_check: last_checked_batch,
            max_transactions_to_check: config.max_transactions_to_check,
        }
    }

    // Set up clients for the main node and all EN instances we want to check.
    fn setup_clients(
        main_node_url: String,
        instances_urls: Vec<String>,
    ) -> (HttpClient, Vec<InstanceHttpClient>) {
        let main_node_client = HttpClientBuilder::default()
            .build(main_node_url)
            .expect("Failed to create an HTTP client for the main node");

        let mut instance_clients: Vec<InstanceHttpClient> = Vec::new();
        for url in instances_urls {
            let client = HttpClientBuilder::default()
                .build(url.clone())
                .expect("Failed to create an HTTP client for an instance of the external node");
            instance_clients.push(InstanceHttpClient { url, client });
        }

        (main_node_client, instance_clients)
    }

    pub async fn run(mut self, stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        match self.mode {
            RpcMode::Triggered => {
                tracing::info!("Starting Checker in Triggered mode");
                if let Err(e) = self.run_triggered().await {
                    self.log_divergences();
                    tracing::error!("Error running in Triggered mode: {:?}", e);
                }
                // Ensure CI will fail if any divergences were found.
                assert!(self.divergences.is_empty(), "Divergences found");
            }
            RpcMode::Continuous => {
                tracing::info!("Starting Checker in Continuous mode");
                if let Err(e) = self.run_continuous(stop_receiver).await {
                    tracing::error!("Error running in Continuous mode: {:?}", e);
                }
            }
        }
        Ok(())
    }

    // For each instance, spawn a task that will continuously poll the instance for new miniblocks
    // and compare them with corresponding main node miniblocks.
    //
    // Errors in task loops exist the loop, stop the tasks, and cause all other tasks to exit too.
    async fn run_continuous(&mut self, mut stop_receiver: Receiver<bool>) -> RpcResult<()> {
        let mut join_handles = Vec::new();

        for instance in &self.instance_clients {
            let main_node_client = self.main_node_client.clone();
            let instance_client = instance.clone();
            let instance_stop_receiver = stop_receiver.clone();
            let mut checker = self.clone();

            let handle = tokio::spawn(async move {
                tracing::info!("Started a task to check instance {}", instance_client.url);
                if let Err(e) = checker.run_node_level_checkers(&instance_client).await {
                    tracing::error!("Error checking instance {}: {:?}", instance_client.url, e);
                };
                let mut next_block_to_check = checker.start_miniblock.unwrap_or(MiniblockNumber(0));

                // - Get the next block the instance has to be checked.
                // - Get the corresponding block from the main node.
                // - Run the checkers through the blocks.
                // - Maybe check batches.
                loop {
                    tracing::debug!(
                        "entered loop to check miniblock #({}) for instance: {}",
                        next_block_to_check,
                        instance_client.url
                    );

                    if *instance_stop_receiver.borrow() {
                        break;
                    }

                    let instance_miniblock = match instance_client
                        .client
                        .get_block_details(next_block_to_check)
                        .await
                    {
                        Ok(Some(miniblock)) => miniblock,
                        Ok(None) => {
                            tracing::debug!(
                                "No miniblock found for miniblock #({}). Sleeping for {} seconds",
                                next_block_to_check,
                                checker.instance_poll_period
                            );
                            // The instance doesn't have a next block to check yet. For now, we wait until it does.
                            // TODO(BFT-165): Implement miniblock existence divergence checker.
                            sleep(Duration::from_secs(checker.instance_poll_period)).await;
                            continue;
                        }
                        Err(e) => {
                            tracing::error!(
                                "Error getting miniblock #({}) from instance: {}: {:?}",
                                next_block_to_check,
                                instance_client.url,
                                e
                            );
                            break;
                        }
                    };

                    let main_node_miniblock = match main_node_client
                        .get_block_details(next_block_to_check)
                        .await
                    {
                        Ok(Some(miniblock)) => miniblock,
                        Ok(None) => {
                            tracing::error!(
                                "Miniblock #({}), which exists in external node instance {}, was not found in the main node",
                                next_block_to_check, instance_client.url
                            );
                            break;
                        }
                        Err(e) => {
                            tracing::error!("Error getting miniblock from main node while checking instance {}: {:?}", instance_client.url, e);
                            break;
                        }
                    };

                    let main_node_miniblock_txs = match checker
                        .create_tx_map(&main_node_client, main_node_miniblock.number)
                        .await
                    {
                        Ok(tx_map) => tx_map,
                        Err(e) => {
                            tracing::error!("Error creating tx map for main node miniblock while checking instance {}: {}", instance_client.url, e);
                            break;
                        }
                    };

                    match checker
                        .compare_miniblocks(
                            &instance_client,
                            &main_node_miniblock_txs,
                            &main_node_miniblock,
                            &instance_miniblock,
                        )
                        .await
                    {
                        Ok(_) => {
                            tracing::info!(
                                "successfully checked miniblock #({}) for instance: {}",
                                next_block_to_check,
                                instance_client.url
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Error checking miniblock #({}) for instance {}: {:?}. Skipping this miniblock",
                                next_block_to_check,
                                instance_client.url,
                                e
                            );
                        }
                    }
                    next_block_to_check += 1;

                    if let Err(e) = checker
                        .maybe_check_batches(&instance_client, instance_miniblock.l1_batch_number)
                        .await
                    {
                        tracing::error!(
                            "Error comparing batch {} for instance {}: {:?}",
                            instance_miniblock.l1_batch_number,
                            instance_client.url,
                            e
                        );
                    }
                }
                Ok(())
            });
            join_handles.push(handle);
        }

        // Wait for either all tasks to finish or a stop signal.
        tokio::select! {
            _ = wait_for_tasks(join_handles, None, None::<futures::future::Ready<()>>, false) => {},
            _ = stop_receiver.changed() => {
                tracing::info!("Stop signal received, shutting down");
            },
        }

        Ok(())
    }

    // Iterate through all miniblocks to be checked. For each, run the checkers through every given instance.
    async fn run_triggered(&mut self) -> RpcResult<()> {
        let start_miniblock = self.start_miniblock.unwrap_or(MiniblockNumber(0));
        let finish_miniblock = match self.finish_miniblock {
            Some(finish_miniblock) => finish_miniblock,
            None => {
                let highest_main_node_miniblock = self.main_node_client.get_block_number().await?;
                MiniblockNumber(highest_main_node_miniblock.as_u32())
            }
        };

        for instance_client in self.instance_clients.clone() {
            self.run_node_level_checkers(&instance_client).await?;
        }

        for miniblock_num_to_check in start_miniblock.0..=finish_miniblock.0 {
            let main_node_miniblock = match self
                .main_node_client
                .get_block_details(MiniblockNumber(miniblock_num_to_check))
                .await
            {
                Ok(Some(miniblock)) => miniblock,
                Ok(None) => panic!("No miniblock found for existing miniblock number {:?}", miniblock_num_to_check),
                Err(e) => panic!("Couldn't fetch existing main node miniblock header for miniblock {:?} due to error: {:?}", miniblock_num_to_check, e),
            };

            let main_node_miniblock_txs = self
                .create_tx_map(&self.main_node_client, main_node_miniblock.number)
                .await?;

            for instance_client in self.instance_clients.clone() {
                let instance_miniblock = match instance_client
                    .client
                    .get_block_details(MiniblockNumber(miniblock_num_to_check))
                    .await?
                {
                    Some(miniblock) => miniblock,
                    None => {
                        // TODO(BFT-165): Implement Miniblock Existence Checker
                        tracing::warn!(
                            "No miniblock found for miniblock #({}) in instance {}. skipping checking it for now.",
                            miniblock_num_to_check,
                            instance_client.url
                        );
                        continue;
                    }
                };

                self.compare_miniblocks(
                    &instance_client,
                    &main_node_miniblock_txs,
                    &main_node_miniblock,
                    &instance_miniblock,
                )
                .await?;

                self.maybe_check_batches(&instance_client, main_node_miniblock.l1_batch_number)
                    .await?;

                tracing::info!(
                    "successfully checked miniblock #({}) for instance: {}",
                    miniblock_num_to_check,
                    instance_client.url
                );
            }
        }

        self.log_divergences();

        Ok(())
    }

    async fn maybe_check_batches(
        &mut self,
        instance_client: &InstanceHttpClient,
        miniblock_batch_number: L1BatchNumber,
    ) -> RpcResult<()> {
        let instance_batch_to_check = self
            .next_batch_to_check
            .get(instance_client.url.as_str())
            .expect("All instance URLs must exists in next_batch_to_check");
        tracing::debug!("Maybe checking batch {}", miniblock_batch_number);

        // We should check batches only the first time we encounter them per instance
        // (i.e., next_instance_batch_to_check == miniblock_batch_number)
        match instance_batch_to_check.cmp(&miniblock_batch_number) {
            Greater => return Ok(()), // This batch has already been checked.
            Less => {
                // Either somehow a batch wasn't checked or a non-genesis miniblock was set as the start
                // miniblock. In the latter case, update the `next_batch_to_check` map and check the batch.
                if self.start_miniblock == Some(MiniblockNumber(0)) {
                    return Err(Error::Custom(format!(
                        "the next batch number to check (#{}) is less than current miniblock batch number (#{}) for instance {}",
                        instance_batch_to_check,
                        miniblock_batch_number,
                        instance_client.url
                    )));
                }
                *self
                    .next_batch_to_check
                    .get_mut(instance_client.url.as_str())
                    .unwrap() = miniblock_batch_number;
            }
            Equal => {}
        }

        let main_node_batch = match self
            .main_node_client
            .get_l1_batch_details(miniblock_batch_number)
            .await
        {
            Ok(Some(batch)) => batch,
            Ok(None) => panic!(
                "No batch found for existing batch with batch number {}",
                miniblock_batch_number
            ),
            Err(e) => panic!(
                "Couldn't fetch existing main node batch for batch number {} due to error: {:?}",
                miniblock_batch_number, e
            ),
        };

        let instance_batch = match instance_client
            .client
            .get_l1_batch_details(miniblock_batch_number)
            .await?
        {
            Some(batch) => batch,
            None => {
                // TODO(BFT-165): Implement batch existence checker.
                tracing::warn!(
                    "No batch found for batch #({}) in instance {}. skipping checking it for now.",
                    miniblock_batch_number,
                    instance_client.url
                );
                return Ok(());
            }
        };

        self.check_batch_details(main_node_batch, instance_batch, &instance_client.url);

        *self
            .next_batch_to_check
            .get_mut(instance_client.url.as_str())
            .unwrap() += 1;

        Ok(())
    }

    // Check divergences using all checkers for every given pair of miniblocks.
    async fn compare_miniblocks(
        &mut self,
        instance_client: &InstanceHttpClient,
        main_node_tx_map: &HashMap<H256, Value>,
        main_node_miniblock: &BlockDetails,
        instance_miniblock: &BlockDetails,
    ) -> RpcResult<()> {
        self.check_miniblock_details(
            &instance_client.url,
            main_node_miniblock,
            instance_miniblock,
        );

        // Also checks tx receipts and tx details
        self.check_transactions(main_node_tx_map, instance_miniblock, instance_client)
            .await?;

        self.check_logs(instance_client, main_node_miniblock.number)
            .await?;

        Ok(())
    }

    // Run all the checkers that ought to be run once per instance (the non block-dependent checkers.)
    async fn run_node_level_checkers(
        &mut self,
        instance_client: &InstanceHttpClient,
    ) -> RpcResult<()> {
        self.check_chain_id(instance_client).await?;
        self.check_main_contract(instance_client).await?;
        self.check_bridge_contracts(instance_client).await?;
        self.check_l1_chain_id(instance_client).await?;
        Ok(())
    }

    // Add a divergence in Triggered mode; log it in Continuous mode.
    fn communicate_divergence(&mut self, url: &str, divergence: Divergence) {
        match self.mode {
            RpcMode::Triggered => {
                // Add a divergence to the list of divergences for the given EN instance.
                let divergences = self.divergences.entry(url.to_string()).or_default();
                divergences.push(divergence.clone());
                tracing::error!("{}", divergence);
            }
            RpcMode::Continuous => {
                // Simply log for now. TODO(BFT-177): Add grafana metrics.
                tracing::error!("{}", divergence);
            }
        }
    }

    // Create a mapping from the tx hash to a json representation of the tx.
    async fn create_tx_map(
        &self,
        client: &HttpClient,
        miniblock_num: MiniblockNumber,
    ) -> RpcResult<HashMap<H256, Value>> {
        let txs = client
            .sync_l2_block(miniblock_num, true)
            .await?
            .and_then(|block| block.transactions)
            .unwrap_or_default();

        let mut tx_map = HashMap::new();
        for tx in txs {
            tx_map.insert(
                tx.hash(),
                serde_json::to_value(tx).expect("tx serialization fail"),
            );
        }

        Ok(tx_map)
    }

    fn log_divergences(&mut self) {
        if self.divergences.is_empty() {
            tracing::info!("No divergences found");
            return;
        }
        for (url, divergences) in &self.divergences {
            tracing::error!("Divergences found for URL: {}", url);
            for divergence in divergences {
                tracing::error!("{}", divergence);
            }
        }
    }
}

// Separate impl for the checkers.
impl Checker {
    fn check_batch_details(
        &mut self,
        main_node_batch: L1BatchDetails,
        instance_batch: L1BatchDetails,
        instance_url: &str,
    ) {
        tracing::debug!(
            "Checking batch details for batch #({})",
            main_node_batch.number
        );
        let batch_differences = compare_json(&main_node_batch, &instance_batch, "".to_string());
        for (key, (main_node_val, instance_val)) in batch_differences {
            self.communicate_divergence(
                instance_url,
                Divergence::BatchDetails(DivergenceDetails {
                    en_instance_url: instance_url.to_string(),
                    main_node_value: Some(format!("{}: {:?}", key, main_node_val)),
                    en_instance_value: Some(format!("{}: {:?}", key, instance_val)),
                    entity_id: Some(format!("Batch Number: {}", main_node_batch.number)),
                    miniblock_number: None,
                }),
            );
        }
    }

    // TODO: What if when we checked the miniblock when the status was Sealed but not Verified?
    fn check_miniblock_details(
        &mut self,
        instance_url: &str,
        main_node_miniblock: &BlockDetails,
        instance_miniblock: &BlockDetails,
    ) {
        tracing::debug!(
            "Checking miniblock details for miniblock #({})",
            main_node_miniblock.number
        );
        let details_differences =
            compare_json(main_node_miniblock, instance_miniblock, "".to_string());
        for (key, (main_node_val, instance_val)) in details_differences {
            self.communicate_divergence(
                instance_url,
                Divergence::MiniblockDetails(DivergenceDetails {
                    en_instance_url: instance_url.to_string(),
                    main_node_value: Some(format!("{}: {:?}", key, main_node_val)),
                    en_instance_value: Some(format!("{}: {:?}", key, instance_val)),
                    entity_id: None,
                    miniblock_number: Some(main_node_miniblock.number),
                }),
            );
        }
    }

    // Looks for txs existing in one node's miniblock and not the other, for
    // discrepancies in the content of txs, and runs the individual transaction checkers.
    async fn check_transactions(
        &mut self,
        main_node_tx_map: &HashMap<H256, Value>,
        instance_miniblock: &BlockDetails,
        instance_client: &InstanceHttpClient,
    ) -> RpcResult<()> {
        let mut instance_tx_map = self
            .create_tx_map(&instance_client.client, instance_miniblock.number)
            .await?;

        tracing::debug!(
            "Checking transactions for miniblock #({}) that has {} transactions",
            instance_miniblock.number,
            instance_tx_map.len(),
        );

        for (i, (tx_hash, main_node_tx)) in main_node_tx_map.iter().enumerate() {
            if let Some(max_num) = self.max_transactions_to_check {
                if i >= max_num {
                    return Ok(());
                }
            }
            match instance_tx_map.remove(tx_hash) {
                Some(instance_tx) => {
                    if *main_node_tx != instance_tx {
                        let tx_differences =
                            compare_json(main_node_tx, &instance_tx, "".to_string());
                        for (key, (main_node_val, instance_val)) in tx_differences {
                            self.communicate_divergence(
                                &instance_client.url,
                                Divergence::Transaction(DivergenceDetails {
                                    en_instance_url: instance_client.url.to_string(),
                                    main_node_value: Some(format!("{}: {:?}", key, main_node_val)),
                                    en_instance_value: Some(format!("{}: {:?}", key, instance_val)),
                                    entity_id: Some(format!("Tx Hash: {}", tx_hash)),
                                    miniblock_number: Some(instance_miniblock.number),
                                }),
                            );
                        }
                    } else {
                        self.check_transaction_receipt(
                            instance_client,
                            tx_hash,
                            instance_miniblock.number,
                        )
                        .await?;

                        self.check_transaction_details(
                            instance_client,
                            tx_hash,
                            instance_miniblock.number,
                        )
                        .await?;
                    }
                }
                None => {
                    self.communicate_divergence(
                        &instance_client.url,
                        Divergence::Transaction(DivergenceDetails {
                            en_instance_url: instance_client.url.to_string(),
                            main_node_value: Some(tx_hash.to_string()),
                            en_instance_value: None,
                            entity_id: Some(format!("Tx Hash: {}", tx_hash)),
                            miniblock_number: Some(instance_miniblock.number),
                        }),
                    );
                    tracing::debug!(
                        "Added divergence for a tx that is in main node but not in instance: {:?}",
                        tx_hash
                    );
                }
            }
        }

        // If there are txs left in the instance tx map, then they don't exist in the main node.
        for tx_hash in instance_tx_map.keys() {
            self.communicate_divergence(
                &instance_client.url,
                Divergence::Transaction(DivergenceDetails {
                    en_instance_url: instance_client.url.to_string(),
                    main_node_value: None,
                    en_instance_value: Some(tx_hash.to_string()),
                    entity_id: Some(format!("Tx Hash: {}", tx_hash)),
                    miniblock_number: Some(instance_miniblock.number),
                }),
            );
            tracing::debug!(
                "Added divergence for a tx that is in instance but not in main node: {:?}",
                tx_hash
            );
        }

        Ok(())
    }

    async fn check_transaction_receipt(
        &mut self,
        instance_client: &InstanceHttpClient,
        tx_hash: &H256,
        miniblock_number: MiniblockNumber,
    ) -> RpcResult<()> {
        tracing::debug!(
            "Checking receipts for a tx in miniblock {}",
            miniblock_number
        );

        let main_node_receipt = self
            .main_node_client
            .get_transaction_receipt(*tx_hash)
            .await?;
        let instance_receipt = instance_client
            .client
            .get_transaction_receipt(*tx_hash)
            .await?;

        let receipt_differences =
            compare_json(&main_node_receipt, &instance_receipt, "".to_string());
        for (key, (main_node_val, instance_val)) in receipt_differences {
            self.communicate_divergence(
                &instance_client.url,
                Divergence::TransactionReceipt(DivergenceDetails {
                    en_instance_url: instance_client.url.to_string(),
                    main_node_value: Some(format!("{}: {:?}", key, main_node_val)),
                    en_instance_value: Some(format!("{}: {:?}", key, instance_val)),
                    entity_id: Some(format!("Tx Hash: {}", tx_hash)),
                    miniblock_number: Some(miniblock_number),
                }),
            );
        }

        Ok(())
    }

    async fn check_transaction_details(
        &mut self,
        instance_client: &InstanceHttpClient,
        tx_hash: &H256,
        miniblock_number: MiniblockNumber,
    ) -> RpcResult<()> {
        tracing::debug!(
            "Checking transaction details for a tx in miniblock {}",
            miniblock_number
        );

        let main_node_tx_details = self
            .main_node_client
            .get_transaction_details(*tx_hash)
            .await?;
        let instance_tx_details = instance_client
            .client
            .get_transaction_details(*tx_hash)
            .await?;

        let tx_details_differences =
            compare_json(&main_node_tx_details, &instance_tx_details, "".to_string());
        for (key, (main_node_val, instance_val)) in tx_details_differences {
            self.communicate_divergence(
                &instance_client.url,
                Divergence::TransactionDetails(DivergenceDetails {
                    en_instance_url: instance_client.url.to_string(),
                    main_node_value: Some(format!("{}: {:?}", key, main_node_val)),
                    en_instance_value: Some(format!("{}: {:?}", key, instance_val)),
                    entity_id: Some(format!("Tx Hash: {}", tx_hash)),
                    miniblock_number: Some(miniblock_number),
                }),
            );
        }

        Ok(())
    }

    async fn check_logs(
        &mut self,
        instance_client: &InstanceHttpClient,
        current_miniblock_block_num: MiniblockNumber,
    ) -> RpcResult<()> {
        let from_block = current_miniblock_block_num
            .0
            .checked_sub(self.log_check_interval);
        let to_block = current_miniblock_block_num.0;

        if from_block < Some(0) || to_block % self.log_check_interval != 0 {
            tracing::debug!("Skipping log check for miniblock {}", to_block);
            return Ok(());
        }
        tracing::debug!(
            "Checking logs for miniblocks {}-{}",
            from_block.unwrap(),
            to_block - 1
        );

        let filter = FilterBuilder::default()
            .set_from_block(BlockNumber::Number(U64::from(from_block.unwrap())))
            .set_to_block(BlockNumber::Number(U64::from(&to_block - 1)))
            .build();

        let main_node_logs = match self.main_node_client.get_logs(filter.clone()).await {
            Ok(logs) => logs,
            Err(e) => {
                // TODO(BFT-192): Be more specific with checking logs
                tracing::error!("Failed to get logs from main node: {}", e);
                return Ok(());
            }
        };
        let instance_logs = match instance_client.client.get_logs(filter).await {
            Ok(logs) => logs,
            Err(e) => {
                // TODO(BFT-192): Be more specific with checking logs
                tracing::error!("Failed to get logs from instance: {}", e);
                return Ok(());
            }
        };

        for (main_node_log, instance_log) in main_node_logs.iter().zip(instance_logs.iter()) {
            let log_differences = compare_json(&main_node_log, &instance_log, "".to_string());
            for (key, (main_node_val, instance_val)) in log_differences {
                self.communicate_divergence(
                    &instance_client.url,
                    Divergence::Log(DivergenceDetails {
                        en_instance_url: instance_client.url.to_string(),
                        main_node_value: Some(format!("{}: {:?}", key, main_node_val)),
                        en_instance_value: Some(format!("{}: {:?}", key, instance_val)),
                        entity_id: None,
                        miniblock_number: Some(MiniblockNumber(
                            main_node_log.block_number.unwrap().as_u32(),
                        )),
                    }),
                );
            }
        }

        Ok(())
    }

    async fn check_main_contract(&mut self, instance_client: &InstanceHttpClient) -> RpcResult<()> {
        let main_node_main_contract = self.main_node_client.get_main_contract().await?;
        let instance_main_contract = instance_client.client.get_main_contract().await?;

        let contract_differences = compare_json(
            &main_node_main_contract,
            &instance_main_contract,
            "".to_string(),
        );
        for (key, (main_node_val, instance_val)) in contract_differences {
            self.communicate_divergence(
                &instance_client.url,
                Divergence::MainContracts(DivergenceDetails {
                    en_instance_url: instance_client.url.to_string(),
                    main_node_value: Some(format!("{} {:?}", key, main_node_val)),
                    en_instance_value: Some(format!("{} {:?}", key, instance_val)),
                    entity_id: None,
                    miniblock_number: None,
                }),
            );
        }

        Ok(())
    }

    async fn check_chain_id(&mut self, instance_client: &InstanceHttpClient) -> RpcResult<()> {
        let main_node_chain_id = self.main_node_client.chain_id().await?;
        let instance_chain_id = instance_client.client.chain_id().await?;

        if main_node_chain_id != instance_chain_id {
            self.communicate_divergence(
                &instance_client.url,
                Divergence::ChainID(DivergenceDetails {
                    en_instance_url: instance_client.url.to_string(),
                    main_node_value: Some(main_node_chain_id),
                    en_instance_value: Some(instance_chain_id),
                    entity_id: None,
                    miniblock_number: None,
                }),
            );
        }

        Ok(())
    }

    async fn check_l1_chain_id(&mut self, instance_client: &InstanceHttpClient) -> RpcResult<()> {
        let main_node_chain_id = self.main_node_client.l1_chain_id().await?;
        let instance_chain_id = instance_client.client.l1_chain_id().await?;

        if main_node_chain_id != instance_chain_id {
            self.communicate_divergence(
                &instance_client.url,
                Divergence::L1ChainID(DivergenceDetails {
                    en_instance_url: instance_client.url.to_string(),
                    main_node_value: Some(main_node_chain_id),
                    en_instance_value: Some(instance_chain_id),
                    entity_id: None,
                    miniblock_number: None,
                }),
            );
        }

        Ok(())
    }

    async fn check_bridge_contracts(
        &mut self,
        instance_client: &InstanceHttpClient,
    ) -> RpcResult<()> {
        let main_node_bridge_contracts = self.main_node_client.get_bridge_contracts().await?;
        let instance_bridge_contracts = instance_client.client.get_bridge_contracts().await?;

        let receipt_differences = compare_json(
            &main_node_bridge_contracts,
            &instance_bridge_contracts,
            "".to_string(),
        );
        for (key, (main_node_val, instance_val)) in receipt_differences {
            self.communicate_divergence(
                &instance_client.url,
                Divergence::BridgeContracts(DivergenceDetails {
                    en_instance_url: instance_client.url.to_string(),
                    main_node_value: Some(format!("{}: {:?}", key, main_node_val)),
                    en_instance_value: Some(format!("{}: {:?}", key, instance_val)),
                    entity_id: None,
                    miniblock_number: None,
                }),
            );
        }

        Ok(())
    }
}
