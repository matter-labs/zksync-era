use crate::{
    config::CheckerConfig,
    divergence::{Divergence, DivergenceDetails},
    helpers::{compare_json, ExponentialBackoff},
};
use anyhow::Context as _;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    select, spawn,
    sync::{watch::Receiver, Mutex as TokioMutex},
    time::timeout,
};
use zksync_types::{web3::types::U64, MiniblockNumber};
use zksync_utils::wait_for_tasks::wait_for_tasks;
use zksync_web3_decl::{
    jsonrpsee::{
        core::{
            client::{Subscription, SubscriptionClientT},
            Error,
        },
        rpc_params,
        ws_client::{WsClient, WsClientBuilder},
    },
    types::{BlockHeader, PubSubResult},
};

const MAX_RETRIES: u32 = 6;
const GRACE_PERIOD: Duration = Duration::from_secs(60);
const SUBSCRIPTION_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct PubSubChecker {
    main_node_url: String,
    instance_urls: Vec<String>,
    /// Time in seconds for a subscription to be active. If `None`, the subscription will run forever.
    subscription_duration: Option<Duration>,
    /// Mapping of block numbers to the block header and the number of instances that still need to
    /// check the corresponding header.  This Hashmap is shared between all threads.
    /// The number of instances is used to determine when to remove the block from the hashmap.
    pub blocks: Arc<TokioMutex<HashMap<U64, (BlockHeader, usize)>>>,
}

impl PubSubChecker {
    pub async fn new(config: CheckerConfig) -> Self {
        let duration = config.subscription_duration.map(Duration::from_secs);
        Self {
            main_node_url: config
                .main_node_ws_url
                .expect("WS URL for the main node has to be provided for PubSub mode."),
            instance_urls: config
                .instances_ws_urls
                .expect("WS URLs for the EN instances have to be provided for PubSub mode."),
            subscription_duration: duration,
            blocks: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }

    pub async fn run(&self, mut stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("Started pubsub checker");

        let mut join_handles = Vec::new();

        let this = self.clone();
        let main_stop_receiver = stop_receiver.clone();
        let handle = spawn(async move {
            tracing::info!("Started a task to subscribe to the main node");
            if let Err(e) = this.subscribe_main(main_stop_receiver).await {
                tracing::error!("Error in main node subscription task: {}", e);
            }
            Ok(())
        });
        join_handles.push(handle);

        let instance_urls = self.instance_urls.clone();
        for instance_url in &instance_urls {
            let this = self.clone();
            let instance_stop_receiver = stop_receiver.clone();
            let url = instance_url.clone();
            let handle = spawn(async move {
                tracing::info!("Started a task to subscribe to instance {}", url);
                this.subscribe_instance(&url, instance_stop_receiver)
                    .await
                    .with_context(|| format!("Error in instance {} subscription task", url))
            });
            join_handles.push(handle);
        }

        select! {
            _ = wait_for_tasks(join_handles, None, None::<futures::future::Ready<()>>, false) => {},
            _ = stop_receiver.changed() => {
                tracing::info!("Stop signal received, shutting down pubsub checker");
            },
        }
        Ok(())
    }

    // Setup a client for the main node, subscribe, and insert incoming pubsub results into the shared hashmap.
    async fn subscribe_main(&self, stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        let client = self.setup_client(&self.main_node_url).await;
        let params = rpc_params!["newHeads"];

        let mut subscription: Subscription<PubSubResult> = client
            .subscribe("eth_subscribe", params, "eth_unsubscribe")
            .await?;

        let start = Instant::now();
        loop {
            if self.check_if_loop_should_break(&stop_receiver, &start, &self.main_node_url) {
                break;
            }

            let no_res_timeout_duration = self.get_timeout_duration(&start);
            let stream_res = timeout(no_res_timeout_duration, subscription.next())
                .await
                .map_err(|_|
                    anyhow::anyhow!(
                        "OperationTimeout: Haven't gotten an item for over {} seconds subscribing to the main node",
                        no_res_timeout_duration.as_secs()
                    )
                )?;
            let pubsub_res = stream_res.ok_or_else(|| anyhow::anyhow!("Stream has ended"))?;

            let (block_header, block_number) = self.extract_block_info(pubsub_res).await?;

            // Secure the lock for the map and insert the new header.
            let mut blocks = self.blocks.lock().await;
            blocks.insert(block_number, (block_header, self.instance_urls.len()));
            tracing::debug!("Inserted block {} to main node map", block_number);
        }

        Ok(())
    }

    // Setup a client for the instance node, subscribe, and compare incoming pubsub results to the main node's.
    async fn subscribe_instance(
        &self,
        url: &str,
        stop_receiver: Receiver<bool>,
    ) -> anyhow::Result<()> {
        let client = self.setup_client(url).await;
        let params = rpc_params!["newHeads"];

        let mut subscription: Subscription<PubSubResult> = client
            .subscribe("eth_subscribe", params, "eth_unsubscribe")
            .await?;

        let start = Instant::now();
        loop {
            if self.check_if_loop_should_break(&stop_receiver, &start, url) {
                break;
            }

            let no_res_timeout_duration = self.get_timeout_duration(&start);
            let stream_res = timeout(no_res_timeout_duration, subscription.next())
                .await
                .map_err(|_|
                    anyhow::anyhow!(
                        "OperationTimeout: Haven't gotten an item for over {} seconds subscribing to instance {}",
                        no_res_timeout_duration.as_secs(), url
                    )
                )?;
            let pubsub_res = stream_res.ok_or_else(|| anyhow::anyhow!("Stream has ended"))?;
            let (instance_block_header, block_number) = self.extract_block_info(pubsub_res).await?;
            tracing::debug!("Got block {} from instance {}", block_number, url);

            // Get the main node block header from the map and update its count.
            // This should be retried because the map not having the block the instance does might
            // just mean the main node subscriber is lagging.
            let backoff = ExponentialBackoff {
                max_retries: MAX_RETRIES,
                base_delay: Duration::from_secs(1),
                retry_message: format!(
                    "block {} is still not present in main node map for instance {}.",
                    block_number, url,
                ),
            };
            let main_node_value = backoff
                // Wait for the block to appear in the main node map.
                .retry(|| {
                    async move {
                        let mut blocks = self.blocks.lock().await;
                        let main_node_value = blocks.get(&block_number).cloned();
                        match main_node_value {
                            Some((header, count)) => {
                                if count > 1 {
                                    blocks.insert(block_number, (header.clone(), count - 1));
                                } else {
                                    blocks.remove(&block_number);
                                }
                                tracing::debug!("Updated blocks map: {:?}", blocks.keys());
                                Some((header, count))
                            }
                            None => None, // Retry
                        }
                    }
                })
                .await;

            // If main node map contained the header, compare main & instance headers.
            match main_node_value {
                Some((main_node_header, _)) => {
                    self.check_headers(&main_node_header, &instance_block_header, url);
                }
                None => {
                    // If the main subscriber starts ahead of an instance subscriber, the map may
                    // start with block X while instance is looking for block X-1, which will never
                    // be in the map. We don't want to log an error for this case.
                    if start.elapsed() > GRACE_PERIOD {
                        tracing::error!(
                            "block {} has not been found in the main node map for instance {} after {} retries",
                            block_number,
                            url,
                            MAX_RETRIES
                        );
                    }
                }
            };
        }

        Ok(())
    }

    fn get_timeout_duration(&self, start: &Instant) -> Duration {
        match self.subscription_duration {
            Some(duration) => std::cmp::min(
                duration.checked_sub(start.elapsed()),
                Some(SUBSCRIPTION_TIMEOUT),
            )
            .unwrap(),
            None => SUBSCRIPTION_TIMEOUT,
        }
    }

    fn check_if_loop_should_break(
        &self,
        stop_receiver: &Receiver<bool>,
        start: &Instant,
        url: &str,
    ) -> bool {
        if *stop_receiver.borrow() {
            tracing::info!("Stop signal received, shutting down pubsub checker");
            return true;
        }
        if let Some(duration) = self.subscription_duration {
            if start.elapsed() > duration {
                tracing::info!("Client {} reached its subscription duration", url);
                return true;
            }
        }
        false
    }

    async fn setup_client(&self, url: &str) -> WsClient {
        WsClientBuilder::default()
            .build(url)
            .await
            .expect("Failed to create a WS client")
    }

    // Extract the block header and block number from the pubsub result that is expected to be a header.
    async fn extract_block_info(
        &self,
        pubsub_res: Result<PubSubResult, Error>,
    ) -> Result<(BlockHeader, U64), anyhow::Error> {
        let PubSubResult::Header(header) = pubsub_res? else {
            return Err(anyhow::anyhow!("Received non-header pubsub result"));
        };

        let Some(block_number) = header.number else {
            return Err(anyhow::anyhow!("Received header without block number."));
        };

        Ok((header, block_number))
    }

    fn check_headers(
        &self,
        main_node_header: &BlockHeader,
        instance_header: &BlockHeader,
        instance_url: &str,
    ) {
        let header_differences = compare_json(&main_node_header, &instance_header, "".to_string());
        if header_differences.is_empty() {
            tracing::info!(
                "No divergences found in header for block {} for instance {}",
                instance_header.number.unwrap().as_u64(),
                instance_url
            );
        }
        for (key, (main_node_val, instance_val)) in header_differences {
            tracing::error!(
                "{}",
                Divergence::PubSubHeader(DivergenceDetails {
                    en_instance_url: instance_url.to_string(),
                    main_node_value: Some(format!("{}: {:?}", key, main_node_val)),
                    en_instance_value: Some(format!("{}: {:?}", key, instance_val)),
                    entity_id: None,
                    miniblock_number: Some(MiniblockNumber(
                        main_node_header.number.unwrap().as_u32()
                    )),
                }),
            );
        }
    }
}
