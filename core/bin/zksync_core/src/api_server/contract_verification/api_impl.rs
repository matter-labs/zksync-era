use std::time::Instant;

use actix_web::{
    web::{self, Json},
    HttpResponse, Result as ActixResult,
};
use serde::Serialize;

use zksync_types::{contract_verification_api::VerificationIncomingRequest, Address};

use super::api_decl::RestApi;

fn ok_json(data: impl Serialize) -> ActixResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(data))
}

impl RestApi {
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
    pub async fn verification(
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
            .contract_verification_dal()
            .is_contract_verified(request.contract_address)
            .await
        {
            return Ok(HttpResponse::BadRequest().body("This contract is already verified"));
        }

        let request_id = storage
            .contract_verification_dal()
            .add_contract_verification_request(request)
            .await
            .unwrap();

        metrics::histogram!("api.contract_verification.call", start.elapsed(), "method" => "contract_verification");
        ok_json(request_id)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn verification_request_status(
        self_: web::Data<Self>,
        id: web::Path<usize>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let status = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .contract_verification_dal()
            .get_verification_request_status(*id)
            .await
            .unwrap();

        metrics::histogram!("api.contract_verification.call", start.elapsed(), "method" => "contract_verification_request_status");
        match status {
            Some(status) => ok_json(status),
            None => Ok(HttpResponse::NotFound().finish()),
        }
    }

    #[tracing::instrument(skip(self_))]
    pub async fn zksolc_versions(self_: web::Data<Self>) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let versions = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .contract_verification_dal()
            .get_zksolc_versions()
            .await
            .unwrap();

        metrics::histogram!("api.contract_verification.call", start.elapsed(), "method" => "contract_verification_zksolc_versions");
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn solc_versions(self_: web::Data<Self>) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let versions = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .contract_verification_dal()
            .get_solc_versions()
            .await
            .unwrap();

        metrics::histogram!("api.contract_verification.call", start.elapsed(), "method" => "contract_verification_solc_versions");
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn zkvyper_versions(self_: web::Data<Self>) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let versions = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .contract_verification_dal()
            .get_zkvyper_versions()
            .await
            .unwrap();

        metrics::histogram!("api.contract_verification.call", start.elapsed(), "method" => "contract_verification_zkvyper_versions");
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn vyper_versions(self_: web::Data<Self>) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let versions = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .contract_verification_dal()
            .get_vyper_versions()
            .await
            .unwrap();

        metrics::histogram!("api.contract_verification.call", start.elapsed(), "method" => "contract_verification_vyper_versions");
        ok_json(versions)
    }

    #[tracing::instrument(skip(self_))]
    pub async fn verification_info(
        self_: web::Data<Self>,
        address: web::Path<Address>,
    ) -> ActixResult<HttpResponse> {
        let start = Instant::now();

        let info = self_
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .contract_verification_dal()
            .get_contract_verification_info(*address)
            .await
            .unwrap();

        metrics::histogram!("api.contract_verification.call", start.elapsed(), "method" => "contract_verification_info");
        match info {
            Some(info) => ok_json(info),
            None => Ok(HttpResponse::NotFound().finish()),
        }
    }
}
