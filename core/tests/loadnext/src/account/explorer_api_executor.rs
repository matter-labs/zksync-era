use std::cmp;
use std::time::Instant;

use futures::SinkExt;
use once_cell::sync::OnceCell;
use rand::{seq::SliceRandom, Rng};
use rand_distr::Distribution;
use rand_distr::Normal;
use reqwest::{Response, StatusCode};
use serde::{de::DeserializeOwned, Deserialize};
use tokio::sync::Semaphore;

use zksync::error::ClientError;
use zksync_types::explorer_api::{
    BlocksQuery, PaginationDirection, PaginationQuery, TransactionsQuery,
};
use zksync_types::{Address, MiniblockNumber, H256};

use crate::account::AccountLifespan;
use crate::command::{ExplorerApiRequest, ExplorerApiRequestType};
use crate::constants::API_REQUEST_TIMEOUT;
use crate::report::{ActionType, ReportBuilder, ReportLabel};

/// Shared semaphore which limits the number of accounts performing
/// API requests at any moment of time.
/// Lazily initialized by the first account accessing it.
static REQUEST_LIMITER: OnceCell<Semaphore> = OnceCell::new();

#[derive(Debug, Clone)]
pub struct ExplorerApiClient {
    pub client: reqwest::Client,
    pub base_url: String,
    pub last_sealed_block_number: Option<MiniblockNumber>,
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
            code => Err(anyhow::anyhow!("Unexpected status code: {}", code)),
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

    pub async fn blocks(
        &mut self,
        query: BlocksQuery,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/blocks", &self.base_url);
        let response = self.client.get(url).query(&query).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn block(&mut self, number: u32) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/block/{}", &self.base_url, number);
        let response = self.client.get(url).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn transaction(&mut self, hash: &H256) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/transaction/{:?}", &self.base_url, hash);
        let response = self.client.get(url).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn transactions(
        &mut self,
        query: TransactionsQuery,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/transactions", &self.base_url);
        let response = self.client.get(url).query(&query).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn account(
        &mut self,
        address: &Address,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/account/{:?}", &self.base_url, address);
        let response = self.client.get(url).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn contract(
        &mut self,
        address: &Address,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/contract/{:?}", &self.base_url, address);
        let response = self.client.get(url).send().await?;
        Self::response_to_result(response).await
    }

    pub async fn token(&mut self, address: &Address) -> anyhow::Result<Option<serde_json::Value>> {
        let url = format!("{}/token/{:?}", &self.base_url, address);
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

    fn random_existed_block(&self) -> Option<MiniblockNumber> {
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
                let from_block = self.random_existed_block();

                // Offset should be less than last_block_number - from, otherwise no blocks will be returned for the request
                // Offset should be less than 9990 because we have a limit for value (limit + offset) <= 10000
                let offset = from_block
                    .map(|bl| {
                        let last_block = self.explorer_client.last_sealed_block_number.unwrap();
                        rand::thread_rng()
                            .gen_range(0..std::cmp::min((last_block - bl.0).0, 9900) as usize)
                    })
                    .unwrap_or_default();

                self.explorer_client
                    .blocks(BlocksQuery {
                        from: from_block,
                        pagination: PaginationQuery {
                            limit: 100,
                            offset,
                            direction: PaginationDirection::Newer,
                        },
                    })
                    .await
                    .map(drop)
            }
            ExplorerApiRequestType::Block => {
                let block = self.random_existed_block().map(|b| *b).unwrap_or(1);
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
                let from_block = self.random_existed_block();
                let offset = self.get_normally_distributed_offset(3000.0, 9900);
                self.explorer_client
                    .transactions(TransactionsQuery {
                        from_block_number: from_block,
                        from_tx_index: None,
                        block_number: None,
                        address: None,
                        account_address: None,
                        contract_address: None,
                        pagination: PaginationQuery {
                            limit: 100,
                            offset,
                            direction: PaginationDirection::Newer,
                        },
                    })
                    .await
                    .map(drop)
            }
            ExplorerApiRequestType::AccountTransactions => {
                let from_block = self.random_existed_block();
                let offset = self.get_normally_distributed_offset(3000.0, 9900);

                self.explorer_client
                    .transactions(TransactionsQuery {
                        from_block_number: from_block,
                        from_tx_index: None,
                        block_number: None,
                        address: None,
                        account_address: Some(self.wallet.wallet.address()),
                        contract_address: None,
                        pagination: PaginationQuery {
                            limit: 100,
                            offset,
                            direction: PaginationDirection::Newer,
                        },
                    })
                    .await
                    .map(drop)
            }
        }
    }

    fn get_normally_distributed_offset(&self, std_dev: f32, limit: usize) -> usize {
        let normal = Normal::new(0.0, std_dev).unwrap();
        let v: f32 = normal.sample(&mut rand::thread_rng());

        let offset = v.abs() as usize;
        cmp::min(offset, limit)
    }

    pub(super) async fn run_explorer_api_requests_task(mut self) {
        // Setup current last block
        let _ = self.explorer_client.network_stats().await;
        loop {
            let semaphore =
                REQUEST_LIMITER.get_or_init(|| Semaphore::new(self.config.sync_api_requests_limit));
            // The number of simultaneous requests is limited by semaphore.
            let permit = semaphore
                .acquire()
                .await
                .expect("static semaphore cannot be closed");

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
                        let error = err.to_string();

                        vlog::error!("API request failed: {:?}, reason: {}", request, error);
                        ReportLabel::ActionFailed { error }
                    }
                }
            };

            let api_action_type = ActionType::from(request.request_type);

            let report = ReportBuilder::default()
                .action(api_action_type)
                .label(label)
                .time(start.elapsed())
                .reporter(self.wallet.wallet.address())
                .finish();
            drop(permit);
            let _ = self.report_sink.send(report).await;
        }
    }
}
