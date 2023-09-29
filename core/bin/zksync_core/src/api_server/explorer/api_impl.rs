use std::time::Instant;

use actix_web::{
    web::{self, Json},
    HttpResponse, Result as ActixResult,
};
use serde::Serialize;

use zksync_types::{
    explorer_api::{
        AccountDetails, AccountType, AddressDetails, BlocksQuery, ContractDetails, EventsQuery,
        L1BatchesQuery, PaginationQuery, TransactionsQuery, VerificationIncomingRequest,
    },
    Address, L1BatchNumber, MiniblockNumber, H256,
};

use super::api_decl::RestApi;

fn ok_json(data: impl Serialize) -> ActixResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(data))
}

impl RestApi {
    #[tracing::instrument(skip(self_))]
    pub async fn network_stats(self_: web::Data<Self>) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let stats = self_.network_stats.read().await;

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "network_stats");
        ok_json(stats)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn address_details(
        self_: web::Data<Self>,
        address: web::Path<Address>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let account_type = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .accounts_dal()
            .get_account_type(*address)
            .await
            .unwrap();
        let response = match account_type {
            AccountType::EOA => ok_json(AddressDetails::Account(
                self_.account_details_inner(address).await,
            )),
            AccountType::Contract => {
                // If account type is a contract, then `contract_details_inner` must return `Some`.
                let contract_details = self_
                    .contract_details_inner(address)
                    .await
                    .expect("Failed to get contract info");
                ok_json(AddressDetails::Contract(contract_details))
            }
        };

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "address_details");
        response
    }

    async fn account_details_inner(&self, address: web::Path<Address>) -> AccountDetails {
        let mut storage = self
            .replica_connection_pool
            .access_storage_tagged("api")
            .await;

        let balances = storage
            .explorer()
            .accounts_dal()
            .get_balances_for_address(*address)
            .await
            .unwrap();
        let (sealed_nonce, verified_nonce) = storage
            .explorer()
            .accounts_dal()
            .get_account_nonces(*address)
            .await
            .unwrap();

        let account_type = storage
            .explorer()
            .accounts_dal()
            .get_account_type(*address)
            .await
            .unwrap();

        AccountDetails {
            address: *address,
            balances,
            sealed_nonce,
            verified_nonce,
            account_type,
        }
    }

    #[tracing::instrument(skip(self_))]
    pub async fn account_details(
        self_: web::Data<Self>,
        address: web::Path<Address>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();
        let account_details = self_.account_details_inner(address).await;
        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "account_details");
        ok_json(account_details)
    }

    async fn contract_details_inner(&self, address: web::Path<Address>) -> Option<ContractDetails> {
        let mut storage = self
            .replica_connection_pool
            .access_storage_tagged("api")
            .await;
        let contract_info = storage
            .explorer()
            .misc_dal()
            .get_contract_info(*address)
            .await
            .unwrap();
        if let Some(contract_info) = contract_info {
            let contract_stats = storage
                .explorer()
                .misc_dal()
                .get_contract_stats(*address)
                .await
                .unwrap();
            let balances = storage
                .explorer()
                .accounts_dal()
                .get_balances_for_address(*address)
                .await
                .unwrap();
            Some(ContractDetails {
                info: contract_info,
                stats: contract_stats,
                balances,
            })
        } else {
            None
        }
    }

    #[tracing::instrument(skip(self_))]
    pub async fn contract_details(
        self_: web::Data<Self>,
        address: web::Path<Address>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let response = match self_.contract_details_inner(address).await {
            Some(contract_details) => ok_json(contract_details),
            None => Ok(HttpResponse::NotFound().finish()),
        };

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "contract_details");
        response
    }

    #[tracing::instrument]
    fn validate_transactions_query(query: TransactionsQuery) -> Result<(), HttpResponse> {
        if query.from_block_number.is_none()
            && query.block_number.is_none()
            && query.from_tx_index.is_some()
        {
            return Err(HttpResponse::BadRequest()
                .body("Can't use `fromTxIndex` without `fromBlockNumber` or `blockNumber`"));
        }
        if query.account_address.is_some() && query.contract_address.is_some() {
            return Err(HttpResponse::BadRequest()
                .body("Can't use both `accountAddress` and `contractAddress`"));
        }

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn validate_pagination_query(&self, pagination: PaginationQuery) -> Result<(), HttpResponse> {
        if pagination.limit > self.api_config.req_entities_limit() {
            return Err(HttpResponse::BadRequest().body(format!(
                "Limit should not exceed {}",
                self.api_config.req_entities_limit()
            )));
        }
        if pagination.offset + pagination.limit > self.api_config.offset_limit() {
            return Err(HttpResponse::BadRequest().body(format!(
                "(offset + limit) should not exceed {}",
                self.api_config.offset_limit()
            )));
        }

        Ok(())
    }

    #[tracing::instrument(skip(self_))]
    pub async fn transaction_pagination(
        self_: web::Data<Self>,
        web::Query(mut query): web::Query<TransactionsQuery>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();
        if let Err(res) = Self::validate_transactions_query(query) {
            return Ok(res);
        }
        if let Err(res) = self_.validate_pagination_query(query.pagination) {
            return Ok(res);
        }

        let mut storage = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await;
        if let Some(address) = query.address {
            match storage
                .explorer()
                .accounts_dal()
                .get_account_type(address)
                .await
                .unwrap()
            {
                AccountType::EOA => query.account_address = Some(address),
                AccountType::Contract => query.contract_address = Some(address),
            }
        }

        let response = if let Some(account_address) = query.account_address {
            // If there is filter by account address
            // we should query transactions from `events` table.
            storage
                .explorer()
                .transactions_dal()
                .get_account_transactions_page(
                    account_address,
                    query.tx_position(),
                    query.block_number,
                    query.pagination,
                    self_.api_config.offset_limit(),
                    self_.l2_erc20_bridge_addr,
                )
                .await
                .unwrap()
        } else {
            // If there is no filter by account address
            // we can query transactions directly from `transactions` table.
            storage
                .explorer()
                .transactions_dal()
                .get_transactions_page(
                    query.tx_position(),
                    query.block_number,
                    query.l1_batch_number,
                    query.contract_address,
                    query.pagination,
                    self_.api_config.offset_limit(),
                    self_.l2_erc20_bridge_addr,
                )
                .await
                .unwrap()
        };

        let query_type = if query.l1_batch_number.is_some() {
            "l1_batch_txs"
        } else if query.block_number.is_some() {
            "block_txs"
        } else if query.account_address.is_some() {
            "account_txs"
        } else if query.contract_address.is_some() {
            "contract_txs"
        } else {
            "all_txs"
        };
        let metric_endpoint_name = format!("transaction_pagination_{}", query_type);
        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => metric_endpoint_name);

        ok_json(response)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn transaction_details(
        self_: web::Data<Self>,
        hash: web::Path<H256>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let tx_details = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .transactions_dal()
            .get_transaction_details(*hash, self_.l2_erc20_bridge_addr)
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "transaction_details");
        match tx_details {
            Some(tx_details) => ok_json(tx_details),
            None => Ok(HttpResponse::NotFound().finish()),
        }
    }

    #[tracing::instrument(skip(self_))]
    pub async fn block_pagination(
        self_: web::Data<Self>,
        web::Query(query): web::Query<BlocksQuery>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();
        if let Err(res) = self_.validate_pagination_query(query.pagination) {
            return Ok(res);
        }

        let blocks = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .blocks_dal()
            .get_blocks_page(query, self_.network_stats.read().await.last_verified)
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "block_pagination");
        ok_json(blocks)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn block_details(
        self_: web::Data<Self>,
        number: web::Path<u32>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let block_details = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .blocks_dal()
            .get_block_details(MiniblockNumber(*number), self_.fee_account_addr)
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "block_details");
        match block_details {
            Some(block_details) => ok_json(block_details),
            None => Ok(HttpResponse::NotFound().finish()),
        }
    }

    #[tracing::instrument(skip(self_))]
    pub async fn l1_batch_pagination(
        self_: web::Data<Self>,
        web::Query(query): web::Query<L1BatchesQuery>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();
        if let Err(res) = self_.validate_pagination_query(query.pagination) {
            return Ok(res);
        }
        let last_verified_miniblock = self_.network_stats.read().await.last_verified;
        let mut storage = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await;

        let last_verified_l1_batch = storage
            .blocks_web3_dal()
            .get_l1_batch_number_of_miniblock(last_verified_miniblock)
            .await
            .unwrap()
            .expect("Verified miniblock must be included in l1 batch");

        let l1_batches = storage
            .explorer()
            .blocks_dal()
            .get_l1_batches_page(query, last_verified_l1_batch)
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "l1_batch_pagination");
        ok_json(l1_batches)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn l1_batch_details(
        self_: web::Data<Self>,
        number: web::Path<u32>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let l1_batch_details = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .blocks_dal()
            .get_l1_batch_details(L1BatchNumber(*number))
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "l1_batch_details");
        match l1_batch_details {
            Some(l1_batch_details) => ok_json(l1_batch_details),
            None => Ok(HttpResponse::NotFound().finish()),
        }
    }

    #[tracing::instrument(skip(self_))]
    pub async fn token_details(
        self_: web::Data<Self>,
        address: web::Path<Address>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let token_details = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .misc_dal()
            .get_token_details(*address)
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "token_details");
        match token_details {
            Some(token_details) => ok_json(token_details),
            None => Ok(HttpResponse::NotFound().finish()),
        }
    }

    #[tracing::instrument(skip(query))]
    fn validate_contract_verification_query(
        query: &VerificationIncomingRequest,
    ) -> Result<(), HttpResponse> {
        if query.source_code_data.compiler_type() != query.compiler_versions.compiler_type() {
            return Err(HttpResponse::BadRequest().body("incorrect compiler versions"));
        }

        Ok(())
    }

    /// Add a contract verification job to the queue if the requested contract wasn't previously verified.
    #[tracing::instrument(skip(self_, request))]
    pub async fn contract_verification(
        self_: web::Data<Self>,
        Json(request): Json<VerificationIncomingRequest>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();
        if let Err(res) = Self::validate_contract_verification_query(&request) {
            return Ok(res);
        }
        let mut storage = self_
            .master_connection_pool
            .access_storage_tagged("api")
            .await;

        if !storage
            .storage_logs_dal()
            .is_contract_deployed_at_address(request.contract_address)
            .await
        {
            return Ok(
                HttpResponse::BadRequest().body("There is no deployed contract on this address")
            );
        }
        if storage
            .explorer()
            .contract_verification_dal()
            .is_contract_verified(request.contract_address)
            .await
        {
            return Ok(HttpResponse::BadRequest().body("This contract is already verified"));
        }

        let request_id = storage
            .explorer()
            .contract_verification_dal()
            .add_contract_verification_request(request)
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "contract_verification");
        ok_json(request_id)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn events_pagination(
        self_: web::Data<Self>,
        web::Query(query): web::Query<EventsQuery>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();
        if let Err(res) = self_.validate_pagination_query(query.pagination) {
            return Ok(res);
        }

        let events = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .events_dal()
            .get_events_page(query, self_.api_config.offset_limit())
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "events_pagination");

        ok_json(events)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn contract_verification_request_status(
        self_: web::Data<Self>,
        id: web::Path<usize>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let status = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .contract_verification_dal()
            .get_verification_request_status(*id)
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "contract_verification_request_status");
        match status {
            Some(status) => ok_json(status),
            None => Ok(HttpResponse::NotFound().finish()),
        }
    }

    #[tracing::instrument(skip(self_))]
    pub async fn contract_verification_zksolc_versions(
        self_: web::Data<Self>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let versions = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .contract_verification_dal()
            .get_zksolc_versions()
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "contract_verification_zksolc_versions");
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn contract_verification_solc_versions(
        self_: web::Data<Self>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let versions = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .contract_verification_dal()
            .get_solc_versions()
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "contract_verification_solc_versions");
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn contract_verification_zkvyper_versions(
        self_: web::Data<Self>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let versions = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .contract_verification_dal()
            .get_zkvyper_versions()
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "contract_verification_zkvyper_versions");
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn contract_verification_vyper_versions(
        self_: web::Data<Self>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let versions = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .contract_verification_dal()
            .get_vyper_versions()
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "contract_verification_vyper_versions");
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn contract_verification_info(
        self_: web::Data<Self>,
        address: web::Path<Address>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let info = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .explorer()
            .contract_verification_dal()
            .get_contract_verification_info(*address)
            .await
            .unwrap();

        metrics::histogram!("api.explorer.call", start.elapsed(), "method" => "contract_verification_info");
        match info {
            Some(info) => ok_json(info),
            None => Ok(HttpResponse::NotFound().finish()),
        }
    }
}
