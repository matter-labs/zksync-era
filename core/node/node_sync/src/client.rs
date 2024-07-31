//! Client abstractions for syncing between the external node and the main node.

use std::fmt;

use async_trait::async_trait;
use zksync_config::GenesisConfig;
use zksync_health_check::{CheckHealth, Health, HealthStatus};
use zksync_system_constants::ACCOUNT_CODE_STORAGE_ADDRESS;
use zksync_types::{
    api::{self, en},
    get_code_key, Address, L2BlockNumber, ProtocolVersionId, H256, U64,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    namespaces::{EnNamespaceClient, EthNamespaceClient, ZksNamespaceClient},
};

/// Client abstracting connection to the main node.
#[async_trait]
pub trait MainNodeClient: 'static + Send + Sync + fmt::Debug {
    async fn fetch_system_contract_by_hash(
        &self,
        hash: H256,
    ) -> EnrichedClientResult<Option<Vec<u8>>>;

    async fn fetch_genesis_contract_bytecode(
        &self,
        address: Address,
    ) -> EnrichedClientResult<Option<Vec<u8>>>;

    async fn fetch_protocol_version(
        &self,
        protocol_version: ProtocolVersionId,
    ) -> EnrichedClientResult<Option<api::ProtocolVersion>>;

    async fn fetch_l2_block_number(&self) -> EnrichedClientResult<L2BlockNumber>;

    async fn fetch_l2_block(
        &self,
        number: L2BlockNumber,
        with_transactions: bool,
    ) -> EnrichedClientResult<Option<en::SyncBlock>>;

    async fn fetch_consensus_genesis(&self) -> EnrichedClientResult<Option<en::ConsensusGenesis>>;

    async fn fetch_genesis_config(&self) -> EnrichedClientResult<GenesisConfig>;

    async fn fetch_attestation_status(&self) -> EnrichedClientResult<en::AttestationStatus>;
}

#[async_trait]
impl MainNodeClient for Box<DynClient<L2>> {
    async fn fetch_system_contract_by_hash(
        &self,
        hash: H256,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        let bytecode = self
            .get_bytecode_by_hash(hash)
            .rpc_context("get_bytecode_by_hash")
            .with_arg("hash", &hash)
            .await?;
        if let Some(bytecode) = &bytecode {
            let actual_bytecode_hash = zksync_utils::bytecode::hash_bytecode(bytecode);
            if actual_bytecode_hash != hash {
                return Err(EnrichedClientError::custom(
                    "Got invalid base system contract bytecode from main node",
                    "get_bytecode_by_hash",
                )
                .with_arg("hash", &hash)
                .with_arg("actual_bytecode_hash", &actual_bytecode_hash));
            }
        }
        Ok(bytecode)
    }

    async fn fetch_genesis_contract_bytecode(
        &self,
        address: Address,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        const GENESIS_BLOCK: api::BlockIdVariant =
            api::BlockIdVariant::BlockNumber(api::BlockNumber::Number(U64([0])));

        let code_key = get_code_key(&address);
        let code_hash = self
            .get_storage_at(
                ACCOUNT_CODE_STORAGE_ADDRESS,
                zksync_utils::h256_to_u256(*code_key.key()),
                Some(GENESIS_BLOCK),
            )
            .rpc_context("get_storage_at")
            .with_arg("address", &address)
            .await?;
        self.get_bytecode_by_hash(code_hash)
            .rpc_context("get_bytecode_by_hash")
            .with_arg("code_hash", &code_hash)
            .await
    }

    async fn fetch_protocol_version(
        &self,
        protocol_version: ProtocolVersionId,
    ) -> EnrichedClientResult<Option<api::ProtocolVersion>> {
        self.get_protocol_version(Some(protocol_version as u16))
            .rpc_context("fetch_protocol_version")
            .with_arg("protocol_version", &protocol_version)
            .await
    }

    async fn fetch_genesis_config(&self) -> EnrichedClientResult<GenesisConfig> {
        self.genesis_config().rpc_context("genesis_config").await
    }

    async fn fetch_l2_block_number(&self) -> EnrichedClientResult<L2BlockNumber> {
        let number = self
            .get_block_number()
            .rpc_context("get_block_number")
            .await?;
        let number = u32::try_from(number)
            .map_err(|err| EnrichedClientError::custom(err, "u32::try_from"))?;
        Ok(L2BlockNumber(number))
    }

    async fn fetch_l2_block(
        &self,
        number: L2BlockNumber,
        with_transactions: bool,
    ) -> EnrichedClientResult<Option<en::SyncBlock>> {
        self.sync_l2_block(number, with_transactions)
            .rpc_context("fetch_l2_block")
            .with_arg("number", &number)
            .with_arg("with_transactions", &with_transactions)
            .await
    }

    async fn fetch_consensus_genesis(&self) -> EnrichedClientResult<Option<en::ConsensusGenesis>> {
        self.consensus_genesis()
            .rpc_context("consensus_genesis")
            .await
    }

    async fn fetch_attestation_status(&self) -> EnrichedClientResult<en::AttestationStatus> {
        self.attestation_status()
            .rpc_context("attestation_status")
            .await
    }
}

/// Main node health check.
#[derive(Debug)]
pub struct MainNodeHealthCheck(Box<DynClient<L2>>);

impl From<Box<DynClient<L2>>> for MainNodeHealthCheck {
    fn from(client: Box<DynClient<L2>>) -> Self {
        Self(client.for_component("main_node_health_check"))
    }
}

#[async_trait]
impl CheckHealth for MainNodeHealthCheck {
    fn name(&self) -> &'static str {
        "main_node_http_rpc"
    }

    async fn check_health(&self) -> Health {
        if let Err(err) = self.0.get_block_number().await {
            tracing::warn!("Health-check call to main node HTTP RPC failed: {err}");
            let details = serde_json::json!({
                "error": err.to_string(),
            });
            return Health::from(HealthStatus::NotReady).with_details(details);
        }
        HealthStatus::Ready.into()
    }
}
