use std::time::Instant;

use rand::seq::IteratorRandom;
use regex::Regex;
use zksync_types::{api, ethabi::Contract, H256, U64};

use super::{Aborted, AccountLifespan};
use crate::{
    command::{ApiRequest, ApiRequestType},
    config::RequestLimiters,
    constants::API_REQUEST_TIMEOUT,
    report::{ApiActionType, ReportBuilder, ReportLabel},
    rng::LoadtestRng,
    sdk::{
        error::{ClientError, RpcError},
        types::FilterBuilder,
        L2EthNamespaceClient,
    },
};

impl AccountLifespan {
    async fn execute_api_request(&mut self, request: ApiRequest) -> Result<(), ClientError> {
        let request_result =
            tokio::time::timeout(API_REQUEST_TIMEOUT, self.execute_api_request_inner(request))
                .await;

        match request_result {
            Ok(result) => result.map_err(Into::into),
            Err(_) => Err(ClientError::OperationTimeout),
        }
    }

    async fn execute_api_request_inner(&mut self, request: ApiRequest) -> Result<(), RpcError> {
        let wallet = &self.wallet.wallet;
        let ApiRequest {
            request_type,
            block_number,
        } = request;

        match request_type {
            ApiRequestType::BlockWithTxs => wallet
                .provider
                .get_block_by_number(block_number, true)
                .await
                .map(drop),
            ApiRequestType::Balance => wallet
                .get_balance(block_number, self.main_l2_token)
                .await
                .map(drop)
                .map_err(|err| match err {
                    ClientError::RpcError(err) => err,
                    err => RpcError::Custom(err.to_string()),
                }),
            ApiRequestType::GetLogs => {
                let topics =
                    random_topics(&self.wallet.test_contract.contract, &mut self.wallet.rng);
                // `run_api_requests_task` checks whether the cell is initialized
                // at every loop iteration and skips logs action if it's not. Thus,
                // it's safe to unwrap it.
                let contract_address = *self.wallet.deployed_contract_address.get().unwrap();

                let to_block_number = match block_number {
                    api::BlockNumber::Number(number) => {
                        api::BlockNumber::Number(number + U64::from(99_u64))
                    }
                    _ => api::BlockNumber::Latest,
                };
                let mut filter = FilterBuilder::default()
                    .set_from_block(block_number)
                    .set_to_block(to_block_number)
                    .set_topics(Some(topics), None, None, None)
                    .set_address(vec![contract_address])
                    .build();

                let response = wallet.provider.get_logs(filter.clone()).await;
                match response {
                    Err(RpcError::Call(err)) => {
                        let re = Regex::new(r"^Query returned more than \d* results\. Try with this block range \[0x([a-fA-F0-9]+), 0x([a-fA-F0-9]+)]\.$").unwrap();
                        if let Some(caps) = re.captures(err.message()) {
                            filter.to_block = Some(api::BlockNumber::Number(
                                U64::from_str_radix(&caps[2], 16).unwrap(),
                            ));
                            wallet.provider.get_logs(filter).await.map(drop)
                        } else {
                            Err(RpcError::Call(err))
                        }
                    }
                    Err(err) => Err(err),
                    Ok(_) => Ok(()),
                }
            }
        }
    }

    pub(super) async fn run_api_requests_task(
        mut self,
        limiters: &RequestLimiters,
    ) -> Result<(), Aborted> {
        loop {
            // The number of simultaneous requests is limited by semaphore.
            let permit = limiters.api_requests.acquire().await.unwrap();
            let request = ApiRequest::random(&self.wallet.wallet, &mut self.wallet.rng).await;
            let start = Instant::now();

            // Skip the action if the contract is not yet initialized for the account.
            let label = if let (ApiRequestType::GetLogs, None) = (
                request.request_type,
                self.wallet.deployed_contract_address.get(),
            ) {
                ReportLabel::skipped("Contract not deployed yet")
            } else {
                let result = self.execute_api_request(request).await;
                match result {
                    Ok(()) => ReportLabel::ActionDone,
                    Err(err) => {
                        tracing::error!("API request failed: {request:?}, reason: {err}");
                        ReportLabel::failed(err.to_string())
                    }
                }
            };
            drop(permit);

            let api_action_type = ApiActionType::from(request);
            let report = ReportBuilder::default()
                .action(api_action_type)
                .label(label)
                .time(start.elapsed())
                .reporter(self.wallet.wallet.address())
                .finish();
            self.send_report(report).await?;
        }
    }
}

pub fn random_topics(contract: &Contract, rng: &mut LoadtestRng) -> Vec<H256> {
    let events_count = contract.events().count();
    let topics_num = (1..=events_count).choose(rng).unwrap();

    contract
        .events()
        .take(topics_num)
        .map(|event| event.signature())
        .collect()
}
