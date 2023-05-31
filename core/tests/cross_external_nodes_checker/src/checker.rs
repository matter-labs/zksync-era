use crate::config::{CheckerConfig, Mode};
use std::{collections::HashMap, fmt, fmt::Debug, time::Duration};
use zksync_types::{
    api::BlockNumber, explorer_api::BlockDetails, web3::types::U64, MiniblockNumber, H256,
};
use zksync_web3_decl::{
    jsonrpsee::{
        core::RpcResult,
        http_client::{HttpClient, HttpClientBuilder},
    },
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
    types::FilterBuilder,
};

use crate::helpers::{compare_json, setup_sigint_handler, wait_for_tasks};
use serde_json::Value;
use tokio::{sync::watch, time::sleep};

#[derive(Debug, Clone)]
pub struct Checker {
    /// 'Triggered' to run once. 'Continuous' to run forever.
    mode: Mode,
    /// Client for interacting with the main node.
    main_node_client: HttpClient,
    /// Client for interacting with the instance nodes.
    instance_clients: Vec<InstanceClient>,
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
}

#[derive(Debug, Clone)]
struct InstanceClient {
    pub url: String,
    pub client: HttpClient,
}

impl Checker {
    pub fn new(config: CheckerConfig) -> Self {
        let (main_node_client, instance_clients) =
            Self::setup_clients(config.main_node_url, config.instances_urls);

        Self {
            mode: config.mode,
            main_node_client,
            instance_clients,
            start_miniblock: config.start_miniblock.map(|n| n.into()),
            finish_miniblock: config.finish_miniblock.map(|n| n.into()),
            instance_poll_period: config.instance_poll_period,
            divergences: HashMap::new(),
            log_check_interval: 1,
        }
    }

    // Set up clients for the main node and all EN instances we want to check.
    fn setup_clients(
        main_node_url: String,
        instances_urls: Vec<String>,
    ) -> (HttpClient, Vec<InstanceClient>) {
        let main_node_client = HttpClientBuilder::default()
            .build(main_node_url)
            .expect("Failed to create an HTTP client for the main node");

        let mut instance_clients: Vec<InstanceClient> = Vec::new();
        for url in instances_urls {
            let client = HttpClientBuilder::default()
                .build(url.clone())
                .expect("Failed to create an HTTP client for an instance of the external node");
            instance_clients.push(InstanceClient { url, client });
        }

        (main_node_client, instance_clients)
    }

    pub async fn run(mut self) -> RpcResult<()> {
        match self.mode {
            Mode::Triggered => {
                vlog::info!("Starting Checker in Triggered mode");
                self.run_triggered().await?;
            }
            Mode::Continuous => {
                vlog::info!("Starting Checker in Continuous mode");
                self.run_continuous().await?;
            }
        }
        Ok(())
    }

    // For each instance, spawn a task that will continuously poll the instance for new miniblocks
    // and compare them with corresponding main node miniblocks.
    //
    // Errors in task loops exist the loop, stop the tasks, and cause all other tasks to exit too.
    async fn run_continuous(&mut self) -> RpcResult<()> {
        let mut join_handles = Vec::new();

        let sigint_receiver = setup_sigint_handler();
        let (stop_sender, stop_receiver) = watch::channel::<bool>(false);

        for instance in &self.instance_clients {
            let main_node_client = self.main_node_client.clone();
            let instance_client = instance.clone();
            let task_stop_receiver = stop_receiver.clone();
            let mut checker = self.clone();

            let handle = tokio::spawn(async move {
                vlog::debug!("Started a task to check instance {}", instance_client.url);
                if let Err(e) = checker.run_node_level_checkers(&instance_client).await {
                    vlog::error!("Error checking instance {}: {:?}", instance_client.url, e);
                };
                let mut next_block_to_check = checker.start_miniblock.unwrap_or(MiniblockNumber(0));
                loop {
                    vlog::debug!(
                        "entered loop to check miniblock #({}) for instance: {}",
                        next_block_to_check,
                        instance_client.url
                    );

                    if *task_stop_receiver.borrow() {
                        break;
                    }

                    let instance_miniblock = match instance_client
                        .client
                        .get_block_details(next_block_to_check)
                        .await
                    {
                        Ok(Some(miniblock)) => miniblock,
                        Ok(None) => {
                            vlog::debug!(
                                "No miniblock found for miniblock #({}). Sleeping for {} seconds",
                                next_block_to_check,
                                checker.instance_poll_period
                            );
                            // The instance doesn't have a next block to check yet. For now, we wait until it does.
                            sleep(Duration::from_secs(checker.instance_poll_period)).await;
                            continue;
                        }
                        Err(e) => {
                            vlog::error!(
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
                            vlog::error!(
                                "Miniblock #({}), which exists in external node instance {}, was not found in the main node",
                                next_block_to_check, instance_client.url
                            );
                            break;
                        }
                        Err(e) => {
                            vlog::error!("Error getting miniblock from main node while checking instance {}: {:?}", instance_client.url, e);
                            break;
                        }
                    };

                    let main_node_miniblock_txs = match checker
                        .create_tx_map(&main_node_client, main_node_miniblock.number)
                        .await
                    {
                        Ok(tx_map) => tx_map,
                        Err(e) => {
                            vlog::error!("Error creating tx map for main node miniblock while checking instance {}: {}", instance_client.url, e);
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
                            vlog::debug!(
                                "successfully checked miniblock #({}) for instance: {}",
                                next_block_to_check,
                                instance_client.url
                            );
                            next_block_to_check += 1;
                        }
                        Err(e) => {
                            vlog::error!(
                                "Error comparing miniblocks for instance {}: {:?}",
                                instance_client.url,
                                e
                            );
                        }
                    }
                }
            });
            join_handles.push(handle);
        }

        // Wait for either all tasks to finish or a stop signal.
        tokio::select! {
            _ = wait_for_tasks(join_handles) => {},
            _ = sigint_receiver => {
                let _ = stop_sender.send(true);
                vlog::info!("Stop signal received, shutting down");
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
                    None => continue,
                };

                self.compare_miniblocks(
                    &instance_client,
                    &main_node_miniblock_txs,
                    &main_node_miniblock,
                    &instance_miniblock,
                )
                .await?;
            }

            vlog::info!(
                "checked divergences for miniblock number {:?}",
                miniblock_num_to_check,
            );
        }

        self.log_divergences();

        Ok(())
    }

    // Check divergences using all checkers for every given pair of miniblocks.
    async fn compare_miniblocks(
        &mut self,
        instance_client: &InstanceClient,
        main_node_tx_map: &HashMap<H256, Value>,
        main_node_miniblock: &BlockDetails,
        instance_miniblock: &BlockDetails,
    ) -> RpcResult<()> {
        self.check_miniblock_details(
            &instance_client.url,
            main_node_miniblock,
            instance_miniblock,
        )
        .await;

        self.check_transactions(main_node_tx_map, instance_miniblock, instance_client)
            .await?;

        self.check_logs(instance_client, main_node_miniblock.number)
            .await?;

        Ok(())
    }

    // Run all the checkers that ought to be run once per instance (the non block-dependent checkers.)
    async fn run_node_level_checkers(&mut self, instance_client: &InstanceClient) -> RpcResult<()> {
        self.check_chain_id(instance_client).await?;
        self.check_main_contract(instance_client).await?;
        self.check_bridge_contracts(instance_client).await?;
        self.check_l1_chain_id(instance_client).await?;
        self.check_confirmed_tokens(instance_client).await?;
        Ok(())
    }

    // Add a divergence in Triggered mode; log it in Continuous mode.
    fn communicate_divergence(&mut self, url: &str, divergence: Divergence) {
        match self.mode {
            Mode::Triggered => {
                // Add a divergence to the list of divergences for the given EN instance.
                let divergences = self
                    .divergences
                    .entry(url.to_string())
                    .or_insert_with(Vec::new);
                divergences.push(divergence);
            }
            Mode::Continuous => {
                // Simply log for now.
                vlog::error!("{}", divergence);
            }
        }
    }

    // Create a mapping from the tx hash to a json representation of the tx.
    async fn create_tx_map(
        &self,
        client: &HttpClient,
        miniblock_num: MiniblockNumber,
    ) -> RpcResult<HashMap<H256, Value>> {
        let txs = client.get_raw_block_transactions(miniblock_num).await?;

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
            vlog::info!("No divergences found");
            return;
        }
        for (url, divergences) in &self.divergences {
            vlog::warn!("Divergences found for URL: {}", url);
            for divergence in divergences {
                vlog::warn!("{}", divergence);
            }
        }
    }
}

// Separate impl for the checkers.
impl Checker {
    async fn check_miniblock_details(
        &mut self,
        instance_url: &str,
        main_node_miniblock: &BlockDetails,
        instance_miniblock: &BlockDetails,
    ) {
        vlog::debug!(
            "Checking miniblock details for miniblock #({})",
            main_node_miniblock.number
        );
        let receipt_differences =
            compare_json(main_node_miniblock, instance_miniblock, "".to_string());
        for (key, (main_node_val, instance_val)) in receipt_differences {
            self.communicate_divergence(
                instance_url,
                Divergence::MiniblockDetails(DivergenceDetails {
                    en_instance_url: instance_url.to_string(),
                    main_node_value: Some(format!("{}: {:?}", key, main_node_val)),
                    en_instance_value: Some(format!("{}: {:?}", key, instance_val)),
                    miniblock_number: main_node_miniblock.number,
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
        instance_client: &InstanceClient,
    ) -> RpcResult<()> {
        vlog::debug!(
            "Checking transactions for miniblock {}",
            instance_miniblock.number
        );

        let mut instance_tx_map = self
            .create_tx_map(&instance_client.client, instance_miniblock.number)
            .await?;

        for (tx_hash, main_node_tx) in main_node_tx_map {
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
                                    miniblock_number: instance_miniblock.number,
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
                            miniblock_number: instance_miniblock.number,
                        }),
                    );
                    vlog::debug!(
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
                    miniblock_number: instance_miniblock.number,
                }),
            );
            vlog::debug!(
                "Added divergence for a tx that is in instance but not in main node: {:?}",
                tx_hash
            );
        }

        Ok(())
    }

    async fn check_transaction_receipt(
        &mut self,
        instance_client: &InstanceClient,
        tx_hash: &H256,
        miniblock_number: MiniblockNumber,
    ) -> RpcResult<()> {
        vlog::debug!(
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
                    miniblock_number,
                }),
            );
        }

        Ok(())
    }

    async fn check_transaction_details(
        &mut self,
        instance_client: &InstanceClient,
        tx_hash: &H256,
        miniblock_number: MiniblockNumber,
    ) -> RpcResult<()> {
        vlog::debug!(
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
                    miniblock_number,
                }),
            );
        }

        Ok(())
    }

    async fn check_logs(
        &mut self,
        instance_client: &InstanceClient,
        current_miniblock_block_num: MiniblockNumber,
    ) -> RpcResult<()> {
        let from_block = current_miniblock_block_num
            .0
            .checked_sub(self.log_check_interval);
        let to_block = current_miniblock_block_num.0;

        if from_block < Some(0) || to_block % self.log_check_interval != 0 {
            vlog::debug!("Skipping log check for miniblock {}", to_block);
            return Ok(());
        }
        vlog::debug!(
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
                vlog::error!("Failed to get logs from main node: {}", e);
                return Ok(());
            }
        };
        let instance_logs = match instance_client.client.get_logs(filter).await {
            Ok(logs) => logs,
            Err(e) => {
                vlog::error!("Failed to get logs from instance: {}", e);
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
                        miniblock_number: MiniblockNumber(
                            main_node_log.block_number.unwrap().as_u32(),
                        ),
                    }),
                );
            }
        }

        Ok(())
    }

    async fn check_main_contract(&mut self, instance_client: &InstanceClient) -> RpcResult<()> {
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
                    miniblock_number: MiniblockNumber(0),
                }),
            );
        }

        Ok(())
    }

    async fn check_chain_id(&mut self, instance_client: &InstanceClient) -> RpcResult<()> {
        let main_node_chain_id = self.main_node_client.chain_id().await?;
        let instance_chain_id = instance_client.client.chain_id().await?;

        if main_node_chain_id != instance_chain_id {
            self.communicate_divergence(
                &instance_client.url,
                Divergence::ChainID(DivergenceDetails {
                    en_instance_url: instance_client.url.to_string(),
                    main_node_value: Some(main_node_chain_id),
                    en_instance_value: Some(instance_chain_id),
                    miniblock_number: MiniblockNumber(0),
                }),
            );
        }

        Ok(())
    }

    async fn check_l1_chain_id(&mut self, instance_client: &InstanceClient) -> RpcResult<()> {
        let main_node_chain_id = self.main_node_client.l1_chain_id().await?;
        let instance_chain_id = instance_client.client.l1_chain_id().await?;

        if main_node_chain_id != instance_chain_id {
            self.communicate_divergence(
                &instance_client.url,
                Divergence::L1ChainID(DivergenceDetails {
                    en_instance_url: instance_client.url.to_string(),
                    main_node_value: Some(main_node_chain_id),
                    en_instance_value: Some(instance_chain_id),
                    miniblock_number: MiniblockNumber(0),
                }),
            );
        }

        Ok(())
    }

    async fn check_bridge_contracts(&mut self, instance_client: &InstanceClient) -> RpcResult<()> {
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
                    miniblock_number: MiniblockNumber(0),
                }),
            );
        }

        Ok(())
    }

    async fn check_confirmed_tokens(&mut self, instance_client: &InstanceClient) -> RpcResult<()> {
        let main_node_confirmed_tokens = self
            .main_node_client
            .get_confirmed_tokens(0, u8::MAX)
            .await?;
        let instance_confirmed_tokens = instance_client
            .client
            .get_confirmed_tokens(0, u8::MAX)
            .await?;

        let receipt_differences = compare_json(
            &main_node_confirmed_tokens,
            &instance_confirmed_tokens,
            "".to_string(),
        );
        for (key, (main_node_val, instance_val)) in receipt_differences {
            self.communicate_divergence(
                &instance_client.url,
                Divergence::ConfirmedTokens(DivergenceDetails {
                    en_instance_url: instance_client.url.to_string(),
                    main_node_value: Some(format!("{}: {:?}", key, main_node_val)),
                    en_instance_value: Some(format!("{}: {:?}", key, instance_val)),
                    miniblock_number: MiniblockNumber(0),
                }),
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Divergence {
    MiniblockDetails(DivergenceDetails<Option<String>>),
    Transaction(DivergenceDetails<Option<String>>),
    TransactionReceipt(DivergenceDetails<Option<String>>),
    TransactionDetails(DivergenceDetails<Option<String>>),
    Log(DivergenceDetails<Option<String>>),
    MainContracts(DivergenceDetails<Option<String>>),
    BridgeContracts(DivergenceDetails<Option<String>>),
    ChainID(DivergenceDetails<Option<U64>>),
    L1ChainID(DivergenceDetails<Option<U64>>),
    ConfirmedTokens(DivergenceDetails<Option<String>>),
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Divergence::MiniblockDetails(details) => {
                write!(f, "Miniblock Details divergence found: {}", details)
            }
            Divergence::Transaction(details) => {
                write!(f, "Transaction divergence found: {}", details)
            }
            Divergence::TransactionReceipt(details) => {
                write!(f, "TransactionReceipt divergence found: {}", details)
            }
            Divergence::TransactionDetails(details) => {
                write!(f, "TransactionDetails divergence found: {}", details)
            }
            Divergence::Log(details) => write!(f, "Log divergence found: {}", details),
            Divergence::MainContracts(details) => {
                write!(f, "MainContracts divergence found: {}", details)
            }
            Divergence::BridgeContracts(details) => {
                write!(f, "BridgeContracts divergence found: {}", details)
            }
            Divergence::ChainID(details) => write!(f, "ChainID divergence found: {}", details),
            Divergence::L1ChainID(details) => write!(f, "L1ChainID divergence found: {}", details),
            Divergence::ConfirmedTokens(details) => {
                write!(f, "ConfirmedTokens divergence found: {}", details)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DivergenceDetails<T> {
    en_instance_url: String,
    main_node_value: T,
    en_instance_value: T,
    miniblock_number: MiniblockNumber,
}

impl<T: fmt::Display> fmt::Display for DivergenceDetails<Option<T>> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let main_node_value = match &self.main_node_value {
            Some(value) => format!("{}", value),
            None => String::from("None"),
        };
        let en_instance_value = match &self.en_instance_value {
            Some(value) => format!("{}", value),
            None => String::from("None"),
        };
        write!(
            f,
            "Main node value: {}, EN instance value: {}, Miniblock number: {} in EN instance: {}",
            main_node_value, en_instance_value, self.miniblock_number, self.en_instance_url
        )
    }
}
