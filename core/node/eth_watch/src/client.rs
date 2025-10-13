use std::{collections::HashMap, fmt, sync::Arc};

use anyhow::Context;
use zksync_contracts::{
    bytecode_supplier_contract, getters_facet_contract, l1_asset_router_contract, l2_message_root,
    state_transition_manager_contract, verifier_contract, wrapped_base_token_store_contract,
};
use zksync_eth_client::{
    clients::{DynClient, L1},
    CallFunctionArgs, ClientError, ContractCallError, EnrichedClientError, EnrichedClientResult,
    EthInterface,
};
use zksync_system_constants::L2_MESSAGE_ROOT_ADDRESS;
use zksync_types::{
    abi::ZkChainSpecificUpgradeData,
    api::{ChainAggProof, Log},
    ethabi::{decode, Contract, ParamType},
    utils::encode_ntv_asset_id,
    web3::{BlockId, BlockNumber, Filter, FilterBuilder},
    Address, L1BatchNumber, L2BlockNumber, L2ChainId, SLChainId, H256,
    SHARED_BRIDGE_ETHER_TOKEN_ADDRESS, U256, U64,
};
use zksync_web3_decl::{
    client::{Network, L2},
    namespaces::{EthNamespaceClient, UnstableNamespaceClient, ZksNamespaceClient},
};

const FFLONK_VERIFIER_TYPE: i32 = 0;

/// Common L1 and L2 client functionality used by [`EthWatch`](crate::EthWatch) and constituent event processors.
#[async_trait::async_trait]
pub trait EthClient: 'static + fmt::Debug + Send + Sync {
    /// Returns events in a given block range.
    async fn get_events(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topic1: Option<H256>,
        topic2: Option<H256>,
        retries_left: usize,
    ) -> EnrichedClientResult<Vec<Log>>;

    /// Returns either finalized L1 block number or block number that satisfies `self.confirmations_for_eth_event` if it's set.
    async fn confirmed_block_number(&self) -> EnrichedClientResult<u64>;

    /// Returns finalized L1 block number.
    async fn finalized_block_number(&self) -> EnrichedClientResult<u64>;

    async fn get_total_priority_txs(&self) -> Result<u64, ContractCallError>;
    /// Returns scheduler verification key hash by verifier address.
    async fn scheduler_vk_hash(&self, verifier_address: Address)
        -> Result<H256, ContractCallError>;
    async fn fflonk_scheduler_vk_hash(
        &self,
        verifier_address: Address,
    ) -> Result<Option<H256>, ContractCallError>;
    /// Returns upgrade diamond cut by packed protocol version.
    async fn diamond_cut_by_version(
        &self,
        packed_version: H256,
    ) -> EnrichedClientResult<Option<Vec<u8>>>;

    async fn get_published_preimages(
        &self,
        hashes: Vec<H256>,
    ) -> EnrichedClientResult<Vec<Option<Vec<u8>>>>;

    async fn get_chain_gateway_upgrade_info(
        &self,
    ) -> Result<Option<ZkChainSpecificUpgradeData>, ContractCallError>;

    /// Returns ID of the chain.
    async fn chain_id(&self) -> EnrichedClientResult<SLChainId>;

    /// Returns chain root for `l2_chain_id` at the moment right after `block_number`.
    /// `block_number` is block number on SL.
    /// `l2_chain_id` is chain id of L2.
    async fn get_chain_root(
        &self,
        block_number: U64,
        l2_chain_id: L2ChainId,
    ) -> Result<H256, ContractCallError>;
}

// This constant is used for reading auxiliary events
const LOOK_BACK_BLOCK_RANGE: u64 = 2_500_000;
pub const RETRY_LIMIT: usize = 5;
const TOO_MANY_RESULTS_INFURA: &str = "query returned more than";
const TOO_MANY_RESULTS_ALCHEMY: &str = "response size exceeded";
const TOO_MANY_RESULTS_RETH: &str = "length limit exceeded";
const TOO_BIG_RANGE_RETH: &str = "query exceeds max block range";
const TOO_MANY_RESULTS_CHAINSTACK: &str = "range limit exceeded";
const REQUEST_REJECTED_503: &str = "Request rejected `503`";

/// Implementation of [`EthClient`] based on HTTP JSON-RPC.
#[derive(Debug, Clone)]
pub struct EthHttpQueryClient<Net: Network> {
    client: Box<DynClient<Net>>,
    diamond_proxy_addr: Address,
    new_upgrade_cut_data_signature: H256,
    bytecode_published_signature: H256,
    bytecode_supplier_addr: Option<Address>,
    wrapped_base_token_store: Option<Address>,
    l1_shared_bridge_addr: Option<Address>,
    l1_message_root_address: Option<Address>,
    // Only present for post-shared bridge chains.
    state_transition_manager_address: Option<Address>,
    server_notifier_address: Option<Address>,
    chain_admin_address: Option<Address>,
    verifier_contract_abi: Contract,
    getters_facet_contract_abi: Contract,
    message_root_abi: Contract,
    l1_asset_router_abi: Contract,
    wrapped_base_token_store_abi: Contract,
    confirmations_for_eth_event: Option<u64>,
    l2_chain_id: L2ChainId,
}

impl<Net: Network> EthHttpQueryClient<Net>
where
    Box<DynClient<Net>>: GetLogsClient,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: Box<DynClient<Net>>,
        diamond_proxy_addr: Address,
        bytecode_supplier_addr: Option<Address>,
        wrapped_base_token_store: Option<Address>,
        l1_shared_bridge_addr: Option<Address>,
        l1_message_root_address: Option<Address>,
        state_transition_manager_address: Option<Address>,
        chain_admin_address: Option<Address>,
        server_notifier_address: Option<Address>,
        confirmations_for_eth_event: Option<u64>,
        l2_chain_id: L2ChainId,
    ) -> Self {
        tracing::debug!(
            "New eth client, ZKsync addr: {:x}, chain_admin_address: {:?}",
            diamond_proxy_addr,
            chain_admin_address
        );
        Self {
            client: client.for_component("watch"),
            diamond_proxy_addr,
            state_transition_manager_address,
            server_notifier_address,
            chain_admin_address,
            bytecode_supplier_addr,
            new_upgrade_cut_data_signature: state_transition_manager_contract()
                .event("NewUpgradeCutData")
                .context("NewUpgradeCutData event is missing in ABI")
                .unwrap()
                .signature(),
            bytecode_published_signature: bytecode_supplier_contract()
                .event("BytecodePublished")
                .context("BytecodePublished event is missing in ABI")
                .unwrap()
                .signature(),
            verifier_contract_abi: verifier_contract(),
            getters_facet_contract_abi: getters_facet_contract(),
            message_root_abi: l2_message_root(),
            l1_asset_router_abi: l1_asset_router_contract(),
            wrapped_base_token_store_abi: wrapped_base_token_store_contract(),
            confirmations_for_eth_event,
            wrapped_base_token_store,
            l1_shared_bridge_addr,
            l1_message_root_address,
            l2_chain_id,
        }
    }

    fn get_default_address_list(&self) -> Vec<Address> {
        let addresses = [
            Some(self.diamond_proxy_addr),
            self.state_transition_manager_address,
            self.chain_admin_address,
            self.server_notifier_address,
            Some(L2_MESSAGE_ROOT_ADDRESS),
            self.l1_message_root_address,
        ];
        addresses.into_iter().flatten().collect()
    }

    #[async_recursion::async_recursion]
    async fn get_events_inner(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topics1: Option<Vec<H256>>,
        topics2: Option<Vec<H256>>,
        addresses: Option<Vec<Address>>,
        retries_left: usize,
    ) -> EnrichedClientResult<Vec<Log>> {
        let mut builder = FilterBuilder::default()
            .from_block(from)
            .to_block(to)
            .topics(topics1.clone(), topics2.clone(), None, None);
        if let Some(addresses) = addresses.clone() {
            builder = builder.address(addresses);
        }
        let filter = builder.build();
        let mut result = self.client.get_logs(filter).await;

        // This code is compatible with both Infura and Alchemy API providers.
        // Note: we don't handle rate-limits here - assumption is that we're never going to hit them.
        if let Err(err) = &result {
            tracing::warn!("Provider returned error message: {err}");
            let err_message = err.as_ref().to_string();
            let err_code = if let ClientError::Call(err) = err.as_ref() {
                Some(err.code())
            } else {
                None
            };

            let should_retry = |err_code, err_message: String| {
                // All of these can be emitted by either API provider.
                err_code == Some(-32603)             // Internal error
                    || err_message.contains("failed")    // Server error
                    || err_message.contains("timed out") // Time-out error
            };

            // check whether the error is related to having too many results
            if err_message.contains(TOO_MANY_RESULTS_INFURA)
                || err_message.contains(TOO_MANY_RESULTS_ALCHEMY)
                || err_message.contains(TOO_MANY_RESULTS_RETH)
                || err_message.contains(TOO_BIG_RANGE_RETH)
                || err_message.contains(TOO_MANY_RESULTS_CHAINSTACK)
                || err_message.contains(REQUEST_REJECTED_503)
                || err.is_timeout()
            {
                // get the numeric block ids
                let from_number = match from {
                    BlockNumber::Number(num) => num,
                    _ => {
                        // invalid variant
                        return result;
                    }
                };
                let to_number = match to {
                    BlockNumber::Number(num) => num,
                    BlockNumber::Latest => self.client.block_number().await?,
                    _ => {
                        // invalid variant
                        return result;
                    }
                };

                // divide range into two halves and recursively fetch them
                let mid = (from_number + to_number) / 2;

                // safety check to prevent infinite recursion (quite unlikely)
                if from_number >= mid {
                    tracing::warn!("Infinite recursion detected while getting events: from_number={from_number:?}, mid={mid:?}");
                    return result;
                }

                tracing::warn!("Splitting block range in half: {from:?} - {mid:?} - {to:?}");
                let mut first_half = self
                    .get_events_inner(
                        from,
                        BlockNumber::Number(mid),
                        topics1.clone(),
                        topics2.clone(),
                        addresses.clone(),
                        RETRY_LIMIT,
                    )
                    .await?;
                let mut second_half = self
                    .get_events_inner(
                        BlockNumber::Number(mid + 1u64),
                        to,
                        topics1,
                        topics2,
                        addresses,
                        RETRY_LIMIT,
                    )
                    .await?;

                first_half.append(&mut second_half);
                result = Ok(first_half);
            } else if should_retry(err_code, err_message) && retries_left > 0 {
                tracing::warn!("Retrying. Retries left: {retries_left}");
                result = self
                    .get_events_inner(from, to, topics1, topics2, addresses, retries_left - 1)
                    .await;
            }
        }

        result
    }
}

#[async_trait::async_trait]
impl<Net: Network> EthClient for EthHttpQueryClient<Net>
where
    Box<DynClient<Net>>: EthInterface + GetLogsClient,
{
    async fn scheduler_vk_hash(
        &self,
        verifier_address: Address,
    ) -> Result<H256, ContractCallError> {
        // New verifier returns the hash of the verification key.
        CallFunctionArgs::new("verificationKeyHash", ())
            .for_contract(verifier_address, &self.verifier_contract_abi)
            .call(&self.client)
            .await
    }

    async fn get_published_preimages(
        &self,
        hashes: Vec<H256>,
    ) -> EnrichedClientResult<Vec<Option<Vec<u8>>>> {
        let Some(bytecode_supplier_addr) = self.bytecode_supplier_addr else {
            return Ok(vec![None; hashes.len()]);
        };

        let to_block = self.client.block_number().await?;
        let from_block = to_block.saturating_sub((LOOK_BACK_BLOCK_RANGE - 1).into());

        let logs = self
            .get_events_inner(
                from_block.into(),
                to_block.into(),
                Some(vec![self.bytecode_published_signature]),
                Some(hashes.clone()),
                Some(vec![bytecode_supplier_addr]),
                RETRY_LIMIT,
            )
            .await?;

        let mut preimages = HashMap::new();
        for log in logs {
            let hash = log.topics[1];
            let preimage = decode(&[ParamType::Bytes], &log.data.0).expect("Invalid encoding");
            assert_eq!(preimage.len(), 1);
            let preimage = preimage[0].clone().into_bytes().unwrap();
            preimages.insert(hash, preimage);
        }

        Ok(hashes
            .into_iter()
            .map(|hash| preimages.get(&hash).cloned())
            .collect())
    }

    async fn get_events(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topic1: Option<H256>,
        topic2: Option<H256>,
        retries_left: usize,
    ) -> EnrichedClientResult<Vec<Log>> {
        self.get_events_inner(
            from,
            to,
            topic1.map(|topic1| vec![topic1]),
            topic2.map(|topic2| vec![topic2]),
            Some(self.get_default_address_list()),
            retries_left,
        )
        .await
    }

    async fn confirmed_block_number(&self) -> EnrichedClientResult<u64> {
        if let Some(confirmations) = self.confirmations_for_eth_event {
            let latest_block_number = self.client.block_number().await?.as_u64();
            Ok(latest_block_number.saturating_sub(confirmations))
        } else {
            self.finalized_block_number().await
        }
    }

    async fn finalized_block_number(&self) -> EnrichedClientResult<u64> {
        let block = self
            .client
            .block(BlockId::Number(BlockNumber::Finalized))
            .await?
            .ok_or_else(|| {
                let err = ClientError::Custom("Finalized block must be present on L1".into());
                EnrichedClientError::new(err, "block")
            })?;
        let block_number = block.number.ok_or_else(|| {
            let err = ClientError::Custom("Finalized block must contain number".into());
            EnrichedClientError::new(err, "block").with_arg("block", &block)
        })?;
        Ok(block_number.as_u64())
    }

    async fn get_total_priority_txs(&self) -> Result<u64, ContractCallError> {
        CallFunctionArgs::new("getTotalPriorityTxs", ())
            .for_contract(self.diamond_proxy_addr, &self.getters_facet_contract_abi)
            .call(&self.client)
            .await
            .map(|x: U256| x.try_into().unwrap())
    }

    async fn fflonk_scheduler_vk_hash(
        &self,
        verifier_address: Address,
    ) -> Result<Option<H256>, ContractCallError> {
        // New verifier returns the hash of the verification key.
        // We are getting function separately to get the second function with the same name, but
        // overriden one
        let function = self
            .verifier_contract_abi
            .functions_by_name("verificationKeyHash")
            .map_err(ContractCallError::Function)?
            .get(1);

        if let Some(function) = function {
            Ok(
                CallFunctionArgs::new("verificationKeyHash", U256::from(FFLONK_VERIFIER_TYPE))
                    .for_contract(verifier_address, &self.verifier_contract_abi)
                    .call_with_function(&self.client, function.clone())
                    .await
                    .ok(),
            )
        } else {
            Ok(None)
        }
    }

    async fn diamond_cut_by_version(
        &self,
        packed_version: H256,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        let Some(state_transition_manager_address) = self.state_transition_manager_address else {
            return Ok(None);
        };

        let to_block = self.client.block_number().await?;
        let from_block = to_block.saturating_sub((LOOK_BACK_BLOCK_RANGE - 1).into());

        let logs = self
            .get_events_inner(
                from_block.into(),
                to_block.into(),
                Some(vec![self.new_upgrade_cut_data_signature]),
                Some(vec![packed_version]),
                Some(vec![state_transition_manager_address]),
                RETRY_LIMIT,
            )
            .await?;

        Ok(logs.into_iter().next().map(|log| log.data.0))
    }

    async fn chain_id(&self) -> EnrichedClientResult<SLChainId> {
        self.client.fetch_chain_id().await
    }

    async fn get_chain_root(
        &self,
        block_number: U64,
        l2_chain_id: L2ChainId,
    ) -> Result<H256, ContractCallError> {
        CallFunctionArgs::new("getChainRoot", U256::from(l2_chain_id.as_u64()))
            .with_block(BlockId::Number(block_number.into()))
            .for_contract(L2_MESSAGE_ROOT_ADDRESS, &self.message_root_abi)
            .call(&self.client)
            .await
    }

    async fn get_chain_gateway_upgrade_info(
        &self,
    ) -> Result<Option<ZkChainSpecificUpgradeData>, ContractCallError> {
        let Some(l1_shared_bridge_addr) = self.l1_shared_bridge_addr else {
            tracing::warn!("l1 shared bridge is not provided!");
            return Ok(None);
        };

        let Some(l1_wrapped_base_token_store) = self.wrapped_base_token_store else {
            tracing::warn!("l1 wrapped base token store is not provided!");
            return Ok(None);
        };

        let l2_chain_id = U256::from(self.l2_chain_id.as_u64());

        // It does not matter whether the l1 shared bridge is an L1AssetRouter or L1Nullifier,
        // either way it supports the "l2BridgeAddress" method.
        let l2_legacy_shared_bridge: Address =
            CallFunctionArgs::new("l2BridgeAddress", l2_chain_id)
                .for_contract(l1_shared_bridge_addr, &self.l1_asset_router_abi)
                .call(&self.client)
                .await?;

        if l2_legacy_shared_bridge == Address::zero() {
            // This state is not completely impossible, but somewhat undesirable.
            // Contracts will still allow the upgrade to go through without
            // the shared bridge, so we will allow it here as well.
            tracing::error!("L2 shared bridge from L1 is empty");
        }

        let l2_predeployed_wrapped_base_token: Address =
            CallFunctionArgs::new("l2WBaseTokenAddress", l2_chain_id)
                .for_contract(
                    l1_wrapped_base_token_store,
                    &self.wrapped_base_token_store_abi,
                )
                .call(&self.client)
                .await?;

        if l2_predeployed_wrapped_base_token == Address::zero() {
            // This state is not completely impossible, but somewhat undesirable.
            // Contracts will still allow the upgrade to go through without
            // the l2 predeployed wrapped base token, so we will allow it here as well.
            tracing::error!("L2 predeployed wrapped base token is empty");
        }

        let base_token_l1_address: Address = CallFunctionArgs::new("getBaseToken", ())
            .for_contract(self.diamond_proxy_addr, &self.getters_facet_contract_abi)
            .call(&self.client)
            .await?;

        let (base_token_name, base_token_symbol) =
            if base_token_l1_address == SHARED_BRIDGE_ETHER_TOKEN_ADDRESS {
                (String::from("Ether"), String::from("ETH"))
            } else {
                // Due to an issue in the upgrade process, the automatically
                // deployed wrapped base tokens will contain generic names
                (String::from("Base Token"), String::from("BT"))
            };

        let base_token_asset_id = encode_ntv_asset_id(
            // Note, that this is correct only for tokens that are being upgraded to the gateway protocol version.
            // The chains that were deployed after it may have tokens with non-L1 base tokens.
            U256::from(self.chain_id().await?.0),
            base_token_l1_address,
        );

        Ok(Some(ZkChainSpecificUpgradeData {
            base_token_asset_id,
            l2_legacy_shared_bridge,
            l2_predeployed_wrapped_base_token,
            base_token_l1_address,
            base_token_name,
            base_token_symbol,
        }))
    }
}

/// Encapsulates `eth_getLogs` calls.
#[async_trait::async_trait]
pub trait GetLogsClient: 'static + fmt::Debug + Send + Sync {
    /// Returns L2 version of [`Log`] with L2-specific fields, e.g. `l1_batch_number`.
    /// L1 clients fill such fields with `None`.
    async fn get_logs(&self, filter: Filter) -> EnrichedClientResult<Vec<Log>>;
}

#[async_trait::async_trait]
impl GetLogsClient for Box<DynClient<L1>> {
    async fn get_logs(&self, filter: Filter) -> EnrichedClientResult<Vec<Log>> {
        Ok(self
            .logs(&filter)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }
}

#[async_trait::async_trait]
impl GetLogsClient for Box<DynClient<L2>> {
    async fn get_logs(&self, filter: Filter) -> EnrichedClientResult<Vec<Log>> {
        EthNamespaceClient::get_logs(self, filter.into())
            .await
            .map_err(|err| EnrichedClientError::new(err, "eth_getLogs"))
    }
}

/// L2 client functionality used by [`EthWatch`](crate::EthWatch) and constituent event processors.
/// Trait extension for [`EthClient`].
#[async_trait::async_trait]
pub trait ZkSyncExtentionEthClient: EthClient {
    fn into_base(self: Arc<Self>) -> Arc<dyn EthClient>;

    async fn get_chain_log_proof(
        &self,
        batch_number: L1BatchNumber,
        chain_id: L2ChainId,
    ) -> EnrichedClientResult<Option<ChainAggProof>>;

    async fn get_chain_log_proof_until_msg_root(
        &self,
        block_number: L2BlockNumber,
        chain_id: L2ChainId,
    ) -> EnrichedClientResult<Option<ChainAggProof>>;

    async fn get_chain_root_l2(
        &self,
        l1_batch_number: L1BatchNumber,
        l2_chain_id: L2ChainId,
    ) -> Result<Option<H256>, ContractCallError>;
}

#[async_trait::async_trait]
impl ZkSyncExtentionEthClient for EthHttpQueryClient<L1> {
    fn into_base(self: Arc<Self>) -> Arc<dyn EthClient> {
        self
    }

    async fn get_chain_log_proof(
        &self,
        _batch_number: L1BatchNumber,
        _chain_id: L2ChainId,
    ) -> EnrichedClientResult<Option<ChainAggProof>> {
        //TODO(EVM-959): Implement it using l1 contracts
        Err(EnrichedClientError::custom(
            "Method is not supported",
            "get_chain_log_proof",
        ))
    }

    async fn get_chain_log_proof_until_msg_root(
        &self,
        _block_number: L2BlockNumber,
        _chain_id: L2ChainId,
    ) -> EnrichedClientResult<Option<ChainAggProof>> {
        //TODO(EVM-959): Implement it using l1 contracts
        Err(EnrichedClientError::custom(
            "Method is not supported",
            "get_chain_log_proof_until_msg_root",
        ))
    }

    async fn get_chain_root_l2(
        &self,
        _l1_batch_number: L1BatchNumber,
        _l2_chain_id: L2ChainId,
    ) -> Result<Option<H256>, ContractCallError> {
        //TODO(EVM-959): Implement it using l1 contracts
        Err(ContractCallError::EthereumGateway(
            EnrichedClientError::custom("Method is not supported", "get_chain_root_l2"),
        ))
    }
}

#[async_trait::async_trait]
impl ZkSyncExtentionEthClient for EthHttpQueryClient<L2> {
    fn into_base(self: Arc<Self>) -> Arc<dyn EthClient> {
        self
    }

    async fn get_chain_log_proof(
        &self,
        batch_number: L1BatchNumber,
        chain_id: L2ChainId,
    ) -> EnrichedClientResult<Option<ChainAggProof>> {
        self.client
            .get_chain_log_proof(batch_number, chain_id)
            .await
            .map_err(|err| EnrichedClientError::new(err, "unstable_getChainLogProof"))
    }

    async fn get_chain_log_proof_until_msg_root(
        &self,
        block_number: L2BlockNumber,
        chain_id: L2ChainId,
    ) -> EnrichedClientResult<Option<ChainAggProof>> {
        self.client
            .get_chain_log_proof_until_msg_root(block_number, chain_id)
            .await
            .map_err(|err| EnrichedClientError::new(err, "unstable_getChainLogProofUntilMsgRoot"))
    }

    async fn get_chain_root_l2(
        &self,
        l1_batch_number: L1BatchNumber,
        l2_chain_id: L2ChainId,
    ) -> Result<Option<H256>, ContractCallError> {
        let l2_block_range = self
            .client
            .get_l2_block_range(l1_batch_number)
            .await
            .map_err(|err| EnrichedClientError::new(err, "zks_getL1BatchBlockRange"))?;
        if let Some((_, l2_block_number)) = l2_block_range {
            self.get_chain_root(l2_block_number, l2_chain_id)
                .await
                .map(Some)
        } else {
            Ok(None)
        }
    }
}
