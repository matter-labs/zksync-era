use futures::{stream, TryStreamExt};
use rand::{seq::SliceRandom, Rng};
use rand_distr::{Distribution, Normal};
use reqwest::{Response, StatusCode};
use serde::{de::DeserializeOwned, Deserialize};

use std::{cmp, str, time::Instant};

use zksync::error::ClientError;
use zksync_types::explorer_api::{
    BlocksQuery, PaginationDirection, PaginationQuery, TransactionsQuery,
};
use zksync_types::{Address, MiniblockNumber, H256};

use super::{Aborted, AccountLifespan};
use crate::{
    command::{ExplorerApiRequest, ExplorerApiRequestType},
    config::RequestLimiters,
    constants::API_REQUEST_TIMEOUT,
    report::{ActionType, ReportBuilder, ReportLabel},
};

#[derive(Debug, Clone)]
pub struct ExplorerApiClient {
    client: reqwest::Client,
    base_url: String,
    last_sealed_block_number: Option<MiniblockNumber>,
}

impl ExplorerApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: reqwest::Client::default(),
            base_url,
            last_sealed_block_number: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct NetworkStats {
    pub last_sealed: MiniblockNumber,
    pub last_verified: MiniblockNumber,
    pub total_transactions: usize,
}

// Client for explorer api, we don't use return values anywhere, so we return just json
impl ExplorerApiClient {
    async fn response_to_result<T: DeserializeOwned>(
        response: Response,
    ) -> anyhow::Result<Option<T>> {
        match response.status() {
            StatusCode::OK => Ok(Some(response.json().await?)),
            StatusCode::NOT_FOUND => Ok(None),
            code => {
                let body = response.bytes().await;
                let body_str = if let Ok(body) = &body {
                    str::from_utf8(body).ok().filter(|body| body.len() < 1_024)
                } else {
                    None
                };
                let (body_sep, body_str) = match body_str {
                    Some(s) => (", body: ", s),
                    None => ("", ""),
                };
                Err(anyhow::anyhow!(
                    "Unexpected status code: {code}{body_sep}{body_str}"
                ))
            }
        }
    }

    pub async fn network_stats(&mut self) -> anyhow::Result<Option<NetworkStats>> {
        let url = format!("{}/network_stats", &self.base_url);
        let response = self.client.get(url).send().await?;
        let result: anyhow::Result<Option<NetworkStats>> = Self::response_to_result(response).await;
        if let Ok(Some(stats)) = result.as_ref() {
            self.last_sealed_block_number = Some(stats.last_sealed);
        }
        result
    }

    pub async fn blocks(&self, query: BlocksQuery) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/blocks", &self.base_url);
        let response = self.client.get(url).query(&query).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn block(&self, number: u32) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/block/{}", &self.base_url, number);
        let response = self.client.get(url).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn transaction(&self, hash: &H256) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/transaction/{:?}", &self.base_url, hash);
        let response = self.client.get(url).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn transactions(
        &self,
        query: TransactionsQuery,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/transactions", &self.base_url);
        let response = self.client.get(url).query(&query).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn account(&self, address: &Address) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/account/{address:?}", &self.base_url);
        let response = self.client.get(url).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn contract(&self, address: &Address) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/contract/{address:?}", &self.base_url);
        let response = self.client.get(url).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn token(&self, address: &Address) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/token/{address:?}", &self.base_url);
        let response = self.client.get(url).send().await?;
        Self::response_to_result(response).await
    }
}

impl AccountLifespan {
    async fn execute_explorer_api_request(
        &mut self,
        request: ExplorerApiRequest,
    ) -> Result<(), anyhow::Error> {
        let request_result = tokio::time::timeout(
            API_REQUEST_TIMEOUT,
            self.execute_explorer_api_request_inner(request),
        )
        .await;

        match request_result {
            Ok(result) => result.map_err(Into::into),
            Err(_) => Err(ClientError::OperationTimeout)?,
        }
    }

    fn random_existing_block(&self) -> Option<MiniblockNumber> {
        self.explorer_client.last_sealed_block_number.map(|number| {
            let num = rand::thread_rng().gen_range(0..number.0);
            MiniblockNumber(num)
        })
    }

    async fn execute_explorer_api_request_inner(
        &mut self,
        request: ExplorerApiRequest,
    ) -> Result<(), anyhow::Error> {
        let ExplorerApiRequest { request_type } = request;

        match request_type {
            ExplorerApiRequestType::NetworkStats => {
                self.explorer_client.network_stats().await.map(drop)
            }
            ExplorerApiRequestType::Blocks => {
                let from_block = self.random_existing_block();
                // Offset should be less than `last_block - from_block`, otherwise no blocks
                // will be returned for the request.
                let mut pagination = Self::random_pagination();
                pagination.offset = if let Some(from_block) = from_block {
                    let last_block = self.explorer_client.last_sealed_block_number.unwrap();
                    cmp::min(pagination.offset, (last_block.0 - from_block.0) as usize)
                } else {
                    0
                };

                self.explorer_client
                    .blocks(BlocksQuery {
                        from: from_block,
                        pagination,
                    })
                    .await
                    .map(drop)
            }
            ExplorerApiRequestType::Block => {
                let block = self.random_existing_block().map(|b| *b).unwrap_or(1);
                self.explorer_client.block(block).await.map(drop)
            }
            ExplorerApiRequestType::Account => self
                .explorer_client
                .account(&self.wallet.wallet.address())
                .await
                .map(drop),
            ExplorerApiRequestType::Transaction => {
                let tx = self
                    .successfully_sent_txs
                    .read()
                    .await
                    .choose(&mut self.wallet.rng)
                    .copied()
                    .expect("We skip such requests  if success_tx is empty");
                self.explorer_client.transaction(&tx).await.map(drop)
            }
            ExplorerApiRequestType::Contract => {
                let contract = self
                    .wallet
                    .deployed_contract_address
                    .get()
                    .expect("We skip such requests if contract is none");
                self.explorer_client.contract(contract).await.map(drop)
            }
            ExplorerApiRequestType::Token => self
                .explorer_client
                .token(&self.main_l2_token)
                .await
                .map(drop),
            ExplorerApiRequestType::Transactions => {
                let from_block = self.random_existing_block();
                self.explorer_client
                    .transactions(TransactionsQuery {
                        from_block_number: from_block,
                        from_tx_index: None,
                        block_number: None,
                        l1_batch_number: None,
                        address: None,
                        account_address: None,
                        contract_address: None,
                        pagination: Self::random_pagination(),
                    })
                    .await
                    .map(drop)
            }
            ExplorerApiRequestType::AccountTransactions => {
                let from_block = self.random_existing_block();
                self.explorer_client
                    .transactions(TransactionsQuery {
                        from_block_number: from_block,
                        from_tx_index: None,
                        block_number: None,
                        l1_batch_number: None,
                        address: None,
                        account_address: Some(self.wallet.wallet.address()),
                        contract_address: None,
                        pagination: Self::random_pagination(),
                    })
                    .await
                    .map(drop)
            }
        }
    }

    fn random_pagination() -> PaginationQuery {
        // These parameters should correspond to pagination validation logic on the server
        // so that we don't get all API requests failing.
        const LIMIT: usize = 50;
        const OFFSET_STD_DEV: f32 = 60.0;
        const MAX_OFFSET: usize = 200;

        PaginationQuery {
            limit: LIMIT,
            offset: Self::normally_distributed_offset(OFFSET_STD_DEV, MAX_OFFSET),
            direction: PaginationDirection::Newer,
        }
    }

    fn normally_distributed_offset(std_dev: f32, limit: usize) -> usize {
        let normal = Normal::new(0.0, std_dev).unwrap();
        let sampled = normal.sample(&mut rand::thread_rng());
        let offset = sampled.abs() as usize;
        cmp::min(offset, limit)
    }

    async fn run_single_request(mut self, limiters: &RequestLimiters) -> Result<(), Aborted> {
        let permit = limiters.explorer_api_requests.acquire().await.unwrap();

        let request = ExplorerApiRequest::random(&mut self.wallet.rng).await;
        let start = Instant::now();
        let mut empty_success_txs = true;
        if request.request_type == ExplorerApiRequestType::Transaction {
            empty_success_txs = self.successfully_sent_txs.read().await.is_empty();
        }

        let label = if let (ExplorerApiRequestType::Contract, None) = (
            request.request_type,
            self.wallet.deployed_contract_address.get(),
        ) {
            ReportLabel::skipped("Contract not deployed yet")
        } else if let (ExplorerApiRequestType::Transaction, true) =
            (request.request_type, empty_success_txs)
        {
            ReportLabel::skipped("No one txs has been submitted yet")
        } else {
            let result = self.execute_explorer_api_request(request).await;
            match result {
                Ok(_) => ReportLabel::ActionDone,
                Err(err) => {
                    vlog::error!("API request failed: {request:?}, reason: {err}");
                    ReportLabel::failed(err.to_string())
                }
            }
        };
        drop(permit);

        let api_action_type = ActionType::from(request.request_type);
        let report = ReportBuilder::default()
            .action(api_action_type)
            .label(label)
            .time(start.elapsed())
            .reporter(self.wallet.wallet.address())
            .finish();
        self.send_report(report).await
    }

    pub(super) async fn run_explorer_api_requests_task(
        mut self,
        limiters: &RequestLimiters,
    ) -> Result<(), Aborted> {
        // Setup current last block
        self.explorer_client.network_stats().await.ok();

        // We use `try_for_each_concurrent` to propagate test abortion, but we cannot
        // rely solely on its concurrency limiter because we need to limit concurrency
        // for all accounts in total, rather than for each account separately.
        let local_limit = (self.config.sync_api_requests_limit / 5).max(1);
        let request = stream::repeat_with(move || Ok(self.clone()));
        request
            .try_for_each_concurrent(local_limit, |this| this.run_single_request(limiters))
            .await
    }
}
