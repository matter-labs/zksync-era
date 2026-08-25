use std::{collections::HashMap, fmt, sync::Arc};

use anyhow::Context;
use zksync_contracts::{
    bytecode_supplier_contract, getters_facet_contract, l1_asset_router_contract, l2_message_root,
    settlement_layer_v31_upgrade_contract, state_transition_manager_contract, verifier_contract,
    wrapped_base_token_store_contract,
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
    h256_to_address, h256_to_u256,
    protocol_version::ProtocolSemanticVersion,
    u256_to_h256,
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
/// Verifier type routed to the Airbender PLONK verifier by the dual verifier contract
/// (see `EraDualVerifier.sol`: 0 = FFLONK, 1 = PLONK, 2 = Airbender PLONK).
const AIRBENDER_PLONK_VERIFIER_TYPE: i32 = 2;

/// Selector of `UnknownVerifierType()`, the error the dual verifier reverts with when asked for a
/// verification key it does not route (see `contracts/l1-contracts/selectors`).
const UNKNOWN_VERIFIER_TYPE_SELECTOR: [u8; 4] = [0xc3, 0x52, 0xbb, 0x73];

/// EIP-1474 "Execution error": the call was executed and reverted. Distinct from the generic server
/// codes, which say nothing about whether the node even ran the call.
const EXECUTION_ERROR_CODE: i32 = 3;

/// Whether a failed `verificationKeyHash` call proves the verifier has no Airbender route, rather
/// than meaning we failed to ask it. Only a definitive EVM answer counts:
///
/// * a revert with `UnknownVerifierType` — a dual verifier that does not route this type;
/// * a revert with no returndata under an execution-error code — an older verifier with no
///   `verificationKeyHash(uint256)` at all.
///
/// Everything else leaves the key unknown and must not be reported as "no key": the caller falls
/// back to the *previous* key and persists it for the new patch, pinning the new protocol version
/// to the old prover generation. Note that an unclassifiable error is not merely a slower path —
/// it propagates as a transient error, and eth_watch retries the same upgrade event forever, so a
/// *permanently* missing route misread as inconclusive wedges upgrade processing and every
/// processor behind it. Hence "no returndata" is matched on the EVM outcome, not on one encoding
/// of it: providers express it as an absent `data` field, as `null`, or as an empty payload
/// (`"0x"`), and some nest the payload one level deeper as `{"data": {"data": "0x…"}}`.
///
/// Limitation: classification uses only the JSON-RPC error object. A provider that reports reverts
/// under a generic code (`-32000`) *and* strips the payload is indistinguishable from one that
/// failed to answer, so it takes the safe branch and eth_watch retries instead of progressing.
/// Matching on error messages would resolve it but is provider-specific and brittle.
fn verifier_lacks_route(err: &ContractCallError) -> bool {
    let ContractCallError::EthereumGateway(err) = err else {
        return false;
    };
    let ClientError::Call(err) = err.as_ref() else {
        return false;
    };
    // An empty revert is definitive only if the node said it executed the call.
    let empty_revert_is_definitive = err.code() == EXECUTION_ERROR_CODE;

    let Some(data) = err.data() else {
        return empty_revert_is_definitive;
    };
    let Ok(data) = serde_json::from_str::<serde_json::Value>(data.get()) else {
        return false;
    };
    let payload = match &data {
        // Standard shape for a reverting `eth_call`: the revert payload, hex-encoded, in `data`.
        serde_json::Value::String(payload) => payload.as_str(),
        // Nested shape, e.g. `{"data": {"data": "0x…", "message": "…"}}`.
        serde_json::Value::Object(fields) => match fields.get("data").and_then(|d| d.as_str()) {
            Some(payload) => payload,
            None => return false,
        },
        serde_json::Value::Null => return empty_revert_is_definitive,
        _ => return false,
    };

    let hex = payload.strip_prefix("0x").unwrap_or(payload);
    if hex.is_empty() {
        // Same EVM outcome as an absent `data` field, just a different encoding.
        return empty_revert_is_definitive;
    }
    hex.get(..8)
        .and_then(|selector| u32::from_str_radix(selector, 16).ok())
        .is_some_and(|selector| selector.to_be_bytes() == UNKNOWN_VERIFIER_TYPE_SELECTOR)
}

/// Protocol version scheduled on the CTM, as read from its `NewProtocolVersion` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledProtocolVersion {
    /// Protocol version the chain is scheduled to upgrade to.
    pub version: ProtocolSemanticVersion,
    /// SL block in which the upgrade was scheduled. The upgrade diamond cut and the verifier for
    /// `version` are emitted in this very block, so it is a lower bound for looking them up.
    pub block_number: u64,
}

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
    /// Returns the Airbender SNARK-wrapper verification key hash by verifier address, or `None`
    /// if the verifier does not (yet) route an Airbender verifier.
    async fn airbender_scheduler_vk_hash(
        &self,
        verifier_address: Address,
    ) -> Result<Option<H256>, ContractCallError>;

    /// Resolves the protocol version scheduled from `old_version`.
    ///
    /// It is read from the `NewProtocolVersion` event emitted by the CTM, where topic1 is the old
    /// protocol version and topic2 is the new protocol version. Both CTM generations emit it.
    async fn scheduled_protocol_version(
        &self,
        old_version: ProtocolSemanticVersion,
    ) -> EnrichedClientResult<Option<ScheduledProtocolVersion>>;

    /// Returns the latest upgrade diamond cut emitted for `version` at or after `from_block`.
    ///
    /// Which version the cut is keyed by depends on the CTM generation, see
    /// [`DecentralizedUpgradesEventProcessor`](crate::event_processors::DecentralizedUpgradesEventProcessor).
    async fn diamond_cut_for_version(
        &self,
        version: ProtocolSemanticVersion,
        from_block: u64,
    ) -> EnrichedClientResult<Option<Vec<u8>>>;

    /// Returns the verifier address set on the CTM for `new_version`, as read from the
    /// `NewProtocolVersionVerifier` event emitted at or after `from_block`.
    ///
    /// Legacy CTMs do not have this event; there the verifier is part of the upgrade calldata and
    /// this returns `None`.
    async fn verifier_address_for_version(
        &self,
        new_version: ProtocolSemanticVersion,
        from_block: u64,
    ) -> EnrichedClientResult<Option<Address>>;

    async fn get_published_preimages(
        &self,
        hashes: Vec<H256>,
    ) -> EnrichedClientResult<Vec<Option<Vec<u8>>>>;

    async fn get_chain_gateway_upgrade_info(
        &self,
    ) -> Result<Option<ZkChainSpecificUpgradeData>, ContractCallError>;

    /// Returns the Bridgehub proxy address saved at startup, if available.
    fn bridgehub_addr(&self) -> Option<Address>;

    /// Calls `getL2UpgradeTxData` on the settlement-layer upgrade helper contract to obtain
    /// chain-specific L2 upgrade tx calldata.
    async fn get_l2_upgrade_tx_data(
        &self,
        init_address: Address,
        existing_tx_data: Vec<u8>,
    ) -> Result<Vec<u8>, ContractCallError>;

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
    new_protocol_version_signature: H256,
    new_protocol_version_verifier_signature: H256,
    bytecode_published_signature: H256,
    bytecode_supplier_addr: Option<Address>,
    wrapped_base_token_store: Option<Address>,
    l1_shared_bridge_addr: Option<Address>,
    l1_message_root_address: Option<Address>,
    // Only present for post-shared bridge chains.
    state_transition_manager_address: Option<Address>,
    server_notifier_address: Option<Address>,
    chain_admin_address: Option<Address>,
    bridgehub_proxy_addr: Option<Address>,
    verifier_contract_abi: Contract,
    getters_facet_contract_abi: Contract,
    message_root_abi: Contract,
    l1_asset_router_abi: Contract,
    settlement_layer_v31_upgrade_abi: Contract,
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
        bridgehub_proxy_addr: Option<Address>,
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
            bridgehub_proxy_addr,
            bytecode_supplier_addr,
            new_upgrade_cut_data_signature: state_transition_manager_contract()
                .event("NewUpgradeCutData")
                .context("NewUpgradeCutData event is missing in ABI")
                .unwrap()
                .signature(),
            new_protocol_version_signature: state_transition_manager_contract()
                .event("NewProtocolVersion")
                .context("NewProtocolVersion event is missing in ABI")
                .unwrap()
                .signature(),
            new_protocol_version_verifier_signature: state_transition_manager_contract()
                .event("NewProtocolVersionVerifier")
                .context("NewProtocolVersionVerifier event is missing in ABI")
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
            settlement_layer_v31_upgrade_abi: settlement_layer_v31_upgrade_contract(),
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

    // `async_recursion` adds its own `#[must_use]` to the boxed future.
    #[allow(clippy::double_must_use)]
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

    /// Fetches CTM events with `topic1 == signature` and `topic2 == packed_version`, ordered from
    /// the oldest to the newest.
    ///
    /// `from_block` bounds the search below; `None` looks back [`LOOK_BACK_BLOCK_RANGE`] blocks. The
    /// upper bound is [`EthClient::confirmed_block_number()`] — the same visibility rule `EthWatch`
    /// applies to the `UpgradeTimestampUpdated` events that trigger these look-ups. Bounding by the
    /// finalized block instead would miss an upgrade that was just scheduled, since finalization
    /// lags the confirmed block whenever `confirmations_for_eth_event` is small.
    ///
    /// Returns an empty vector if the CTM address is not configured.
    async fn ctm_events_for_version(
        &self,
        signature: H256,
        packed_version: U256,
        from_block: Option<u64>,
        method_name: &'static str,
    ) -> EnrichedClientResult<Vec<Log>> {
        let Some(state_transition_manager_address) = self.state_transition_manager_address else {
            return Ok(vec![]);
        };

        let to_block = self.confirmed_block_number().await.map_err(|e| {
            EnrichedClientError::custom(
                format!("Failed to get confirmed block number: err {e}"),
                method_name,
            )
        })?;
        let from_block =
            from_block.unwrap_or_else(|| to_block.saturating_sub(LOOK_BACK_BLOCK_RANGE));

        let mut logs = self
            .get_events_inner(
                from_block.into(),
                to_block.into(),
                Some(vec![signature]),
                Some(vec![u256_to_h256(packed_version)]),
                Some(vec![state_transition_manager_address]),
                RETRY_LIMIT,
            )
            .await?;
        logs.sort_by_key(|log| {
            (
                log.block_number.unwrap_or_default(),
                log.log_index.unwrap_or_default(),
            )
        });
        Ok(logs)
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

    async fn airbender_scheduler_vk_hash(
        &self,
        verifier_address: Address,
    ) -> Result<Option<H256>, ContractCallError> {
        // Same overloaded `verificationKeyHash(uint256)` as for FFLONK, routed to the Airbender
        // PLONK verifier.
        let function = self
            .verifier_contract_abi
            .functions_by_name("verificationKeyHash")
            .map_err(ContractCallError::Function)?
            .get(1);

        let Some(function) = function else {
            return Ok(None);
        };
        let result = CallFunctionArgs::new(
            "verificationKeyHash",
            U256::from(AIRBENDER_PLONK_VERIFIER_TYPE),
        )
        .for_contract(verifier_address, &self.verifier_contract_abi)
        .call_with_function(&self.client, function.clone())
        .await;

        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(err) if verifier_lacks_route(&err) => Ok(None),
            Err(err) => Err(err),
        }
    }

    async fn scheduled_protocol_version(
        &self,
        old_version: ProtocolSemanticVersion,
    ) -> EnrichedClientResult<Option<ScheduledProtocolVersion>> {
        let logs = self
            .ctm_events_for_version(
                self.new_protocol_version_signature,
                old_version.pack(),
                None,
                "scheduled_protocol_version",
            )
            .await?;

        // If several upgrades were scheduled from the same old version, the one emitted in the
        // latest block corresponds to the currently active upgrade path.
        let Some(log) = logs.last() else {
            return Ok(None);
        };
        let new_version = log.topics.get(2).ok_or_else(|| {
            EnrichedClientError::custom(
                "NewProtocolVersion event is missing the new version topic",
                "scheduled_protocol_version",
            )
        })?;
        let version = ProtocolSemanticVersion::try_from_packed(h256_to_u256(*new_version))
            .map_err(|err| {
                EnrichedClientError::custom(
                    format!("Failed to parse new protocol version: {err}"),
                    "scheduled_protocol_version",
                )
            })?;
        let block_number = log
            .block_number
            .ok_or_else(|| {
                EnrichedClientError::custom(
                    "NewProtocolVersion event is missing the block number",
                    "scheduled_protocol_version",
                )
            })?
            .as_u64();
        Ok(Some(ScheduledProtocolVersion {
            version,
            block_number,
        }))
    }

    async fn diamond_cut_for_version(
        &self,
        version: ProtocolSemanticVersion,
        from_block: u64,
    ) -> EnrichedClientResult<Option<Vec<u8>>> {
        let logs = self
            .ctm_events_for_version(
                self.new_upgrade_cut_data_signature,
                version.pack(),
                Some(from_block),
                "diamond_cut_for_version",
            )
            .await?;

        // The cut may be rewritten after it was first scheduled, in which case the latest event
        // holds the cut that will actually be applied.
        Ok(logs.into_iter().next_back().map(|log| log.data.0))
    }

    async fn verifier_address_for_version(
        &self,
        new_version: ProtocolSemanticVersion,
        from_block: u64,
    ) -> EnrichedClientResult<Option<Address>> {
        let logs = self
            .ctm_events_for_version(
                self.new_protocol_version_verifier_signature,
                new_version.pack(),
                Some(from_block),
                "verifier_address_for_version",
            )
            .await?;

        // If the verifier was set several times, the last event matches the current
        // `protocolVersionVerifier` mapping value.
        let Some(log) = logs.last() else {
            return Ok(None);
        };
        let verifier = log.topics.get(2).ok_or_else(|| {
            EnrichedClientError::custom(
                "NewProtocolVersionVerifier event is missing the verifier topic",
                "verifier_address_for_version",
            )
        })?;
        Ok(Some(h256_to_address(verifier)))
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

    fn bridgehub_addr(&self) -> Option<Address> {
        self.bridgehub_proxy_addr
    }

    async fn get_l2_upgrade_tx_data(
        &self,
        init_address: Address,
        existing_tx_data: Vec<u8>,
    ) -> Result<Vec<u8>, ContractCallError> {
        let bridgehub_addr = self.bridgehub_addr().ok_or_else(|| {
            ContractCallError::EthereumGateway(EnrichedClientError::custom(
                "Bridgehub address is required for v31 upgrade",
                "get_l2_upgrade_tx_data",
            ))
        })?;

        CallFunctionArgs::new(
            "getL2UpgradeTxData",
            (
                bridgehub_addr,
                U256::from(self.l2_chain_id.as_u64()),
                // eth_watch in this binary is bound to an Era chain; Era diamonds
                // pass `false` for the shared v31 settlement-layer helper.
                false,
                existing_tx_data,
            ),
        )
        .for_contract(init_address, &self.settlement_layer_v31_upgrade_abi)
        .call(&self.client)
        .await
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

#[cfg(test)]
mod tests {
    use zksync_web3_decl::jsonrpsee::types::{
        error::{INTERNAL_ERROR_CODE, SERVER_IS_BUSY_CODE},
        ErrorObject,
    };

    use super::*;

    fn call_error(inner: ClientError) -> ContractCallError {
        ContractCallError::EthereumGateway(EnrichedClientError::new(inner, "verificationKeyHash"))
    }

    fn reverted_with(data: &str) -> ContractCallError {
        call_error(ClientError::Call(ErrorObject::owned(
            EXECUTION_ERROR_CODE,
            "execution reverted",
            Some(data),
        )))
    }

    /// The two EVM answers that do mean "no Airbender route": the `UnknownVerifierType` revert,
    /// and an empty revert from a verifier without `verificationKeyHash(uint256)`. Providers
    /// encode the latter in several ways, all of which must classify the same — misreading one as
    /// inconclusive stalls eth_watch permanently.
    #[test]
    fn a_definitive_revert_means_the_verifier_lacks_the_route() {
        assert!(verifier_lacks_route(&reverted_with("0xc352bb73")));
        // Payload nested one level deeper.
        assert!(verifier_lacks_route(&call_error(ClientError::Call(
            ErrorObject::owned(
                EXECUTION_ERROR_CODE,
                "execution reverted",
                Some(serde_json::json!({ "data": "0xc352bb73" })),
            ),
        ))));
        for empty_revert in [
            // No `data` field at all (geth).
            call_error(ClientError::Call(ErrorObject::owned(
                EXECUTION_ERROR_CODE,
                "execution reverted",
                None::<()>,
            ))),
            // `data` present but null.
            call_error(ClientError::Call(ErrorObject::owned(
                EXECUTION_ERROR_CODE,
                "execution reverted",
                Some(serde_json::Value::Null),
            ))),
            // Empty payload, with and without the hex prefix.
            reverted_with("0x"),
            reverted_with(""),
            call_error(ClientError::Call(ErrorObject::owned(
                EXECUTION_ERROR_CODE,
                "execution reverted",
                Some(serde_json::json!({ "data": "0x" })),
            ))),
        ] {
            assert!(
                verifier_lacks_route(&empty_revert),
                "empty revert must mean a missing route: {empty_revert}"
            );
        }
    }

    /// Anything else leaves the key unknown and must propagate, so eth_watch retries rather than
    /// registering the previous prover generation's key for the new version.
    #[test]
    fn an_inconclusive_failure_is_not_a_missing_route() {
        let inconclusive = [
            call_error(ClientError::Transport("connection refused".into())),
            call_error(ClientError::RequestTimeout),
            // An overloaded node answers, but with a retriable error rather than a revert.
            call_error(ClientError::Call(ErrorObject::owned(
                SERVER_IS_BUSY_CODE,
                "server is busy",
                None::<()>,
            ))),
            call_error(ClientError::Call(ErrorObject::owned(
                INTERNAL_ERROR_CODE,
                "internal error",
                None::<()>,
            ))),
            // A revert carrying some *other* custom error.
            reverted_with("0xdeadbeef"),
            // `Error(string)`-encoded `require` message.
            reverted_with("0x08c379a0"),
            // Generic server code with the payload stripped: not classifiable.
            call_error(ClientError::Call(ErrorObject::owned(
                -32000,
                "execution reverted",
                None::<()>,
            ))),
            // Likewise for an empty payload: the code says nothing about whether the call ran.
            call_error(ClientError::Call(ErrorObject::owned(
                -32000,
                "execution reverted",
                Some("0x"),
            ))),
            // A `data` object that carries no payload at all.
            call_error(ClientError::Call(ErrorObject::owned(
                EXECUTION_ERROR_CODE,
                "execution reverted",
                Some(serde_json::json!({ "message": "execution reverted" })),
            ))),
        ];
        for err in inconclusive {
            assert!(
                !verifier_lacks_route(&err),
                "must not be mistaken for a missing route: {err}"
            );
        }
    }
}
