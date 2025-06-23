use std::collections::HashMap;

use tokio::sync::watch;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{
    tee_dcap_collateral_dal::{
        PendingCollateral, PendingFieldCollateral, PendingTcbInfoCollateral,
    },
    Connection, ConnectionPool, Core, CoreDal,
};
use zksync_eth_client::{BoundEthInterface, CallFunctionArgs, ContractCallError, EthInterface};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_l1_contract_interface::{
    i_executor::{
        commit::kzg::{KzgInfo, ZK_SYNC_BYTES_PER_BLOB},
        methods::CommitBatches,
    },
    multicall3::{Multicall3Call, Multicall3Result},
    Tokenizable, Tokenize,
};
use zksync_shared_metrics::BlockL1Stage;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    commitment::{L1BatchWithMetadata, SerializeCommitment},
    eth_sender::{EthTx, EthTxBlobSidecar, EthTxBlobSidecarV1, SidecarBlobV1},
    ethabi::{Function, Token},
    l2_to_l1_log::UserL2ToL1Log,
    protocol_version::{L1VerifierConfig, PACKED_SEMVER_MINOR_MASK},
    pubdata_da::PubdataSendingMode,
    server_notification::GatewayMigrationState,
    settlement::SettlementLayer,
    web3::{contract::Error as Web3ContractError, CallRequest},
    Address, L2ChainId, ProtocolVersionId, SLChainId, H256, U256,
};

use super::aggregated_operations::AggregatedOperation;
use crate::{
    aggregator::OperationSkippingRestrictions,
    health::{EthTxAggregatorHealthDetails, EthTxDetails},
    metrics::{PubdataKind, METRICS},
    publish_criterion::L1GasCriterion,
    zksync_functions::ZkSyncFunctions,
    Aggregator, EthSenderError,
};

#[derive(Debug)]
pub struct DAValidatorPair {
    l1_validator: Address,
    l2_validator: Address,
}

/// Data queried from L1 using multicall contract.
#[derive(Debug)]
#[allow(dead_code)]
pub struct MulticallData {
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub verifier_address: Address,
    pub chain_protocol_version_id: ProtocolVersionId,
    /// The latest validator timelock that is stored on the StateTransitionManager (ChainTypeManager).
    /// For a smoother upgrade process, if the `stm_protocol_version_id` is the same as `chain_protocol_version_id`,
    /// we will use the validator timelock from the CTM. This removes the need to immediately set the correct
    /// validator timelock in the config. However, it is expected that it will be done eventually.
    pub stm_validator_timelock_address: Address,
    pub stm_protocol_version_id: ProtocolVersionId,
    pub da_validator_pair: DAValidatorPair,
}

/// The component is responsible for aggregating l1 batches into eth_txs.
///
/// Such as CommitBlocks, PublishProofBlocksOnchain and ExecuteBlock
/// These eth_txs will be used as a queue for generating signed txs and send them later
#[derive(Debug)]
pub struct EthTxAggregator {
    aggregator: Aggregator,
    eth_client: Box<dyn BoundEthInterface>,
    config: SenderConfig,
    config_timelock_contract_address: Address,
    l1_multicall3_address: Address,
    pub(super) state_transition_chain_contract: Address,
    state_transition_manager_address: Address,
    functions: ZkSyncFunctions,
    rollup_chain_id: L2ChainId,
    /// If set to `Some` node is operating in the 4844 mode with two operator
    /// addresses at play: the main one and the custom address for sending commit
    /// transactions. The `Some` then contains client for this custom operator address.
    eth_client_blobs: Option<Box<dyn BoundEthInterface>>,
    eth_client_tee_dcap: Option<Box<dyn BoundEthInterface>>,
    pool: ConnectionPool<Core>,
    sl_chain_id: SLChainId,
    health_updater: HealthUpdater,
    priority_tree_start_index: Option<usize>,
    settlement_layer: Option<SettlementLayer>,
    initial_pending_nonces: HashMap<Address, u64>,
}

struct TxData {
    calldata: Vec<u8>,
    sidecar: Option<EthTxBlobSidecar>,
}

const FFLONK_VERIFIER_TYPE: i32 = 1;

impl EthTxAggregator {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: ConnectionPool<Core>,
        config: SenderConfig,
        aggregator: Aggregator,
        eth_client: Box<dyn BoundEthInterface>,
        eth_client_blobs: Option<Box<dyn BoundEthInterface>>,
        eth_client_tee_dcap: Option<Box<dyn BoundEthInterface>>,
        config_timelock_contract_address: Address,
        state_transition_manager_address: Address,
        l1_multicall3_address: Address,
        state_transition_chain_contract: Address,
        rollup_chain_id: L2ChainId,
        settlement_layer: Option<SettlementLayer>,
    ) -> Self {
        let eth_client = eth_client.for_component("eth_tx_aggregator");
        let eth_client_blobs = eth_client_blobs.map(|c| c.for_component("eth_tx_aggregator"));
        let eth_client_tee_dcap = eth_client_tee_dcap.map(|c| c.for_component("eth_tx_aggregator"));

        let functions = ZkSyncFunctions::default();

        let mut initial_pending_nonces = HashMap::new();
        for client in eth_client_blobs
            .iter()
            .chain(std::iter::once(&eth_client))
            .chain(eth_client_tee_dcap.iter())
        {
            let address = client.sender_account();
            let nonce = client.pending_nonce().await.unwrap().as_u64();

            initial_pending_nonces.insert(address, nonce);
        }

        let sl_chain_id = (*eth_client).as_ref().fetch_chain_id().await.unwrap();

        Self {
            config,
            aggregator,
            eth_client,
            config_timelock_contract_address,
            state_transition_manager_address,
            l1_multicall3_address,
            state_transition_chain_contract,
            functions,
            rollup_chain_id,
            eth_client_blobs,
            eth_client_tee_dcap,
            pool,
            sl_chain_id,
            health_updater: ReactiveHealthCheck::new("eth_tx_aggregator").1,
            priority_tree_start_index: None,
            settlement_layer,
            initial_pending_nonces,
        }
    }

    pub async fn run(mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.health_updater
            .update(Health::from(HealthStatus::Ready));

        tracing::info!(
            "Initialized eth_tx_aggregator with is_pre_fflonk_verifier: {:?}",
            self.config.is_verifier_pre_fflonk
        );

        let pool = self.pool.clone();
        loop {
            let mut storage = pool.connection_tagged("eth_sender").await.unwrap();

            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth_tx_aggregator is shutting down");
                break;
            }

            if let Err(err) = self.loop_iteration(&mut storage).await {
                // Web3 API request failures can cause this,
                // and anything more important is already properly reported.
                tracing::warn!("eth_sender error {err:?}");
            }

            tokio::time::sleep(self.config.aggregate_tx_poll_period).await;
        }
        Ok(())
    }

    pub(super) async fn get_multicall_data(&mut self) -> Result<MulticallData, EthSenderError> {
        let (calldata, evm_emulator_hash_requested) = self.generate_calldata_for_multicall();
        let args = CallFunctionArgs::new(&self.functions.aggregate3.name, calldata).for_contract(
            self.l1_multicall3_address,
            &self.functions.multicall_contract,
        );
        let aggregate3_result: Token = args.call((*self.eth_client).as_ref()).await?;
        self.parse_multicall_data(aggregate3_result, evm_emulator_hash_requested)
    }

    // Multicall's aggregate function accepts 1 argument - arrays of different contract calls.
    // The role of the method below is to tokenize input for multicall, which is actually a vector of tokens.
    // Each token describes a specific contract call.
    pub(super) fn generate_calldata_for_multicall(&self) -> (Vec<Token>, bool) {
        const ALLOW_FAILURE: bool = false;

        // First zksync contract call
        let get_l2_bootloader_hash_input = self
            .functions
            .get_l2_bootloader_bytecode_hash
            .encode_input(&[])
            .unwrap();
        let get_bootloader_hash_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_l2_bootloader_hash_input,
        };

        // Second zksync contract call
        let get_l2_default_aa_hash_input = self
            .functions
            .get_l2_default_account_bytecode_hash
            .encode_input(&[])
            .unwrap();
        let get_default_aa_hash_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_l2_default_aa_hash_input,
        };

        // Third zksync contract call
        let get_verifier_params_input = self
            .functions
            .get_verifier_params
            .encode_input(&[])
            .unwrap();
        let get_verifier_params_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_verifier_params_input,
        };

        // Fourth zksync contract call
        let get_verifier_input = self.functions.get_verifier.encode_input(&[]).unwrap();
        let get_verifier_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_verifier_input,
        };

        // Fifth zksync contract call
        let get_protocol_version_input = self
            .functions
            .get_protocol_version
            .encode_input(&[])
            .unwrap();
        let get_protocol_version_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_protocol_version_input,
        };

        let get_stm_protocol_version_input = self
            .functions
            .state_transition_manager_contract
            .function("protocolVersion")
            .unwrap()
            .encode_input(&[])
            .unwrap();
        let get_stm_protocol_version_call = Multicall3Call {
            target: self.state_transition_manager_address,
            allow_failure: ALLOW_FAILURE,
            calldata: get_stm_protocol_version_input,
        };

        let get_stm_validator_timelock_input = self
            .functions
            .state_transition_manager_contract
            .function("validatorTimelock")
            .unwrap()
            .encode_input(&[])
            .unwrap();
        let get_stm_validator_timelock_call = Multicall3Call {
            target: self.state_transition_manager_address,
            allow_failure: ALLOW_FAILURE,
            calldata: get_stm_validator_timelock_input,
        };

        let get_da_validator_pair_input = self
            .functions
            .get_da_validator_pair
            .encode_input(&[])
            .unwrap();

        let get_da_validator_pair_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_da_validator_pair_input,
        };

        let mut token_vec = vec![
            get_bootloader_hash_call.into_token(),
            get_default_aa_hash_call.into_token(),
            get_verifier_params_call.into_token(),
            get_verifier_call.into_token(),
            get_protocol_version_call.into_token(),
            get_stm_protocol_version_call.into_token(),
            get_stm_validator_timelock_call.into_token(),
            get_da_validator_pair_call.into_token(),
        ];

        let mut evm_emulator_hash_requested = false;
        let get_l2_evm_emulator_hash_input = self
            .functions
            .get_evm_emulator_bytecode_hash
            .as_ref()
            .and_then(|f| f.encode_input(&[]).ok());
        if let Some(input) = get_l2_evm_emulator_hash_input {
            let call = Multicall3Call {
                target: self.state_transition_chain_contract,
                allow_failure: ALLOW_FAILURE,
                calldata: input,
            };
            token_vec.insert(2, call.into_token());
            evm_emulator_hash_requested = true;
        }

        (token_vec, evm_emulator_hash_requested)
    }

    // The role of the method below is to de-tokenize multicall call's result, which is actually a token.
    // This token is an array of tuples like `(bool, bytes)`, that contain the status and result for each contract call.
    pub(super) fn parse_multicall_data(
        &self,
        token: Token,
        evm_emulator_hash_requested: bool,
    ) -> Result<MulticallData, EthSenderError> {
        let parse_error = |tokens: &[Token]| {
            Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                format!("Failed to parse multicall token: {:?}", tokens),
            )))
        };

        if let Token::Array(call_results) = token {
            let number_of_calls = if evm_emulator_hash_requested { 9 } else { 8 };
            // 8 or 9 calls are aggregated in multicall
            if call_results.len() != number_of_calls {
                return parse_error(&call_results);
            }
            let mut call_results_iterator = call_results.into_iter();

            let multicall3_bootloader =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;

            if multicall3_bootloader.len() != 32 {
                return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                    format!(
                        "multicall3 bootloader hash data is not of the len of 32: {:?}",
                        multicall3_bootloader
                    ),
                )));
            }
            let bootloader = H256::from_slice(&multicall3_bootloader);

            let multicall3_default_aa =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;
            if multicall3_default_aa.len() != 32 {
                return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                    format!(
                        "multicall3 default aa hash data is not of the len of 32: {:?}",
                        multicall3_default_aa
                    ),
                )));
            }
            let default_aa = H256::from_slice(&multicall3_default_aa);

            let evm_emulator = if evm_emulator_hash_requested {
                let multicall3_evm_emulator =
                    Multicall3Result::from_token(call_results_iterator.next().unwrap())?
                        .return_data;
                if multicall3_evm_emulator.len() != 32 {
                    return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                        format!(
                            "multicall3 EVM emulator hash data is not of the len of 32: {:?}",
                            multicall3_evm_emulator
                        ),
                    )));
                }
                Some(H256::from_slice(&multicall3_evm_emulator))
            } else {
                None
            };

            let base_system_contracts_hashes = BaseSystemContractsHashes {
                bootloader,
                default_aa,
                evm_emulator,
            };

            call_results_iterator.next().unwrap(); // FIXME: why is this value requested?

            let verifier_address =
                Self::parse_address(call_results_iterator.next().unwrap(), "verifier address")?;

            let chain_protocol_version_id = Self::parse_protocol_version(
                call_results_iterator.next().unwrap(),
                "contract protocol version",
            )?;
            let stm_protocol_version_id = Self::parse_protocol_version(
                call_results_iterator.next().unwrap(),
                "STM protocol version",
            )?;
            let stm_validator_timelock_address = Self::parse_address(
                call_results_iterator.next().unwrap(),
                "STM validator timelock address",
            )?;

            let da_validator_pair = Self::parse_da_validator_pair(
                call_results_iterator.next().unwrap(),
                "contract DA validator pair",
            )?;

            return Ok(MulticallData {
                base_system_contracts_hashes,
                verifier_address,
                chain_protocol_version_id,
                stm_protocol_version_id,
                stm_validator_timelock_address,
                da_validator_pair,
            });
        }
        parse_error(&[token])
    }

    fn parse_protocol_version(
        data: Token,
        name: &'static str,
    ) -> Result<ProtocolVersionId, EthSenderError> {
        let multicall_data = Multicall3Result::from_token(data)?.return_data;
        if multicall_data.len() != 32 {
            return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                format!(
                    "multicall3 {name} data is not of the len of 32: {:?}",
                    multicall_data
                ),
            )));
        }

        let protocol_version = U256::from_big_endian(&multicall_data);
        // In case the protocol version is smaller than `PACKED_SEMVER_MINOR_MASK`, it will mean that it is
        // equal to the `protocol_version_id` value, since it the interface from before the semver was supported.
        let protocol_version_id = if protocol_version < U256::from(PACKED_SEMVER_MINOR_MASK) {
            ProtocolVersionId::try_from(protocol_version.as_u32() as u16).unwrap()
        } else {
            ProtocolVersionId::try_from_packed_semver(protocol_version).unwrap()
        };

        Ok(protocol_version_id)
    }

    fn parse_address(data: Token, name: &'static str) -> Result<Address, EthSenderError> {
        let multicall_data = Multicall3Result::from_token(data)?.return_data;
        if multicall_data.len() != 32 {
            return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                format!(
                    "multicall3 {name} data is not of the len of 32: {:?}",
                    multicall_data
                ),
            )));
        }

        Ok(Address::from_slice(&multicall_data[12..]))
    }

    fn parse_da_validator_pair(
        data: Token,
        name: &'static str,
    ) -> Result<DAValidatorPair, EthSenderError> {
        // In the first word of the output, the L1 DA validator is present
        const L1_DA_VALIDATOR_OFFSET: usize = 12;
        // In the second word of the output, the L2 DA validator is present
        const L2_DA_VALIDATOR_OFFSET: usize = 32 + 12;

        let multicall_data = Multicall3Result::from_token(data)?.return_data;
        if multicall_data.len() != 64 {
            return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                format!(
                    "multicall3 {name} data is not of the len of 32: {:?}",
                    multicall_data
                ),
            )));
        }

        let pair = DAValidatorPair {
            l1_validator: Address::from_slice(&multicall_data[L1_DA_VALIDATOR_OFFSET..32]),
            l2_validator: Address::from_slice(&multicall_data[L2_DA_VALIDATOR_OFFSET..64]),
        };

        Ok(pair)
    }

    fn timelock_contract_address(
        &self,
        chain_protocol_version_id: ProtocolVersionId,
        stm_protocol_version_id: ProtocolVersionId,
        stm_validator_timelock_address: Address,
    ) -> Address {
        // For chains before v26 (gateway) we use the timelock address from config.
        // After that, the timelock address can be fetched from STM as it is the valid one
        // for versions starting from v26 and is not expected to change in the near future.
        if chain_protocol_version_id < ProtocolVersionId::gateway_upgrade() {
            self.config_timelock_contract_address
        } else {
            assert!(
                chain_protocol_version_id <= stm_protocol_version_id,
                "Chain upgraded before STM"
            );

            stm_validator_timelock_address
        }
    }

    /// Loads current verifier config on L1
    async fn get_snark_wrapper_vk_hash(
        &mut self,
        verifier_address: Address,
    ) -> Result<H256, EthSenderError> {
        let get_vk_hash = &self.functions.verification_key_hash;

        let vk_hash: H256 = CallFunctionArgs::new(&get_vk_hash.name, ())
            .for_contract(verifier_address, &self.functions.verifier_contract)
            .call((*self.eth_client).as_ref())
            .await?;
        Ok(vk_hash)
    }

    /// Returns whether there is a pending gateway upgrade.
    /// During gateway upgrade, the signature of the `executeBatches` function on `ValidatorTimelock` will change.
    /// This means that transactions that were created before the upgrade but were sent right after it
    /// will fail, which we want to avoid.
    async fn is_pending_gateway_upgrade(
        storage: &mut Connection<'_, Core>,
        chain_protocol_version: ProtocolVersionId,
    ) -> bool {
        // If the gateway protocol version is present in the DB, and its timestamp is larger than `now`, it means that
        // the upgrade process on the server has begun.
        // However, if the protocol version on the contract is lower than the `gateway_upgrade`, it means that the upgrade has
        // not yet completed.

        if storage
            .blocks_dal()
            .pending_protocol_version()
            .await
            .unwrap()
            < ProtocolVersionId::gateway_upgrade()
        {
            return false;
        }

        chain_protocol_version < ProtocolVersionId::gateway_upgrade()
    }

    async fn get_fflonk_snark_wrapper_vk_hash(
        &mut self,
        verifier_address: Address,
    ) -> Result<Option<H256>, EthSenderError> {
        let get_vk_hash = &self.functions.verification_key_hash;
        // We are getting function separately to get the second function with the same name, but
        // overriden one
        let function = self
            .functions
            .verifier_contract
            .functions_by_name(&get_vk_hash.name)
            .map_err(|x| EthSenderError::ContractCall(ContractCallError::Function(x)))?
            .get(1);

        if let Some(function) = function {
            let vk_hash: Option<H256> =
                CallFunctionArgs::new(&get_vk_hash.name, U256::from(FFLONK_VERIFIER_TYPE))
                    .for_contract(verifier_address, &self.functions.verifier_contract)
                    .call_with_function((*self.eth_client).as_ref(), function.clone())
                    .await
                    .ok();
            Ok(vk_hash)
        } else {
            Ok(None)
        }
    }

    #[tracing::instrument(skip_all, name = "EthTxAggregator::loop_iteration")]
    async fn loop_iteration(
        &mut self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EthSenderError> {
        let gateway_migration_state = self.gateway_status(storage).await;
        let MulticallData {
            base_system_contracts_hashes,
            verifier_address,
            chain_protocol_version_id,
            stm_protocol_version_id,
            stm_validator_timelock_address,
            da_validator_pair,
        } = self.get_multicall_data().await.map_err(|err| {
            tracing::error!("Failed to get multicall data {err:?}");
            err
        })?;

        let snark_wrapper_vk_hash = self
            .get_snark_wrapper_vk_hash(verifier_address)
            .await
            .map_err(|err| {
                tracing::error!("Failed to get VK hash from the Verifier {err:?}");
                err
            })?;
        let fflonk_snark_wrapper_vk_hash = self
            .get_fflonk_snark_wrapper_vk_hash(verifier_address)
            .await
            .map_err(|err| {
                tracing::error!("Failed to get FFLONK VK hash from the Verifier {err:?}");
                err
            })?;

        let l1_verifier_config = L1VerifierConfig {
            snark_wrapper_vk_hash,
            fflonk_snark_wrapper_vk_hash,
        };

        let priority_tree_start_index =
            if let Some(priority_tree_start_index) = self.priority_tree_start_index {
                Some(priority_tree_start_index)
            } else {
                self.priority_tree_start_index =
                    get_priority_tree_start_index(self.eth_client.as_ref()).await?;
                self.priority_tree_start_index
            };
        let commit_restriction = self
            .config
            .tx_aggregation_only_prove_and_execute
            .then_some("tx_aggregation_only_prove_and_execute=true");

        let mut op_restrictions = OperationSkippingRestrictions {
            commit_restriction,
            prove_restriction: None,
            execute_restriction: Self::is_pending_gateway_upgrade(
                storage,
                chain_protocol_version_id,
            )
            .await
            .then_some("there is a pending gateway upgrade"),
        };

        // When migrating to or from gateway, the DA validator pair will be reset and so the chain should not
        // send new commit transactions before the da validator pair is updated
        if da_validator_pair.l1_validator == Address::zero()
            || da_validator_pair.l2_validator == Address::zero()
        {
            let reason = Some("DA validator pair is not set on the settlement layer");
            op_restrictions.commit_restriction = reason;
            // We only disable commit operations, the rest are allowed
        }

        if self.config.tx_aggregation_paused {
            let reason = Some("tx aggregation is paused");
            op_restrictions.commit_restriction = reason;
            op_restrictions.prove_restriction = reason;
            op_restrictions.execute_restriction = reason;
        }

        if gateway_migration_state == GatewayMigrationState::InProgress {
            let reason = Some("Gateway migration started");
            op_restrictions.commit_restriction = reason;
            // For the migration from gateway to L1, we need to wait for all blocks to be executed
            if let None | Some(SettlementLayer::L1(_)) = self.settlement_layer {
                op_restrictions.prove_restriction = reason;
                op_restrictions.execute_restriction = reason;
            }
        }

        if let Some(agg_op) = self
            .aggregator
            .get_next_ready_operation(
                storage,
                base_system_contracts_hashes,
                chain_protocol_version_id,
                l1_verifier_config,
                op_restrictions,
                priority_tree_start_index,
            )
            .await?
        {
            let is_gateway = self.is_gateway();
            let tx = self
                .save_eth_tx(
                    storage,
                    &agg_op,
                    self.timelock_contract_address(
                        chain_protocol_version_id,
                        stm_protocol_version_id,
                        stm_validator_timelock_address,
                    ),
                    chain_protocol_version_id,
                    is_gateway,
                )
                .await?;
            Self::report_eth_tx_saving(storage, &agg_op, &tx).await;

            self.health_updater.update(
                EthTxAggregatorHealthDetails {
                    last_saved_tx: EthTxDetails::new(&tx, None),
                }
                .into(),
            );
        }

        // FIXME: TEE - might want a TeeAggregator?
        self.aggregate_tee_dcap_transactions(storage).await?;
        self.aggregate_tee_attestations(storage).await?;
        self.aggregate_tee_proofs(storage).await?;
        Ok(())
    }

    // FIXME: TEE
    async fn aggregate_tee_proofs(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EthSenderError> {
        let pending = storage
            .tee_proof_generation_dal()
            .get_tee_proofs_for_eth_sender()
            .await
            .map_err(|_e| {
                // FIXME: TEE
                EthSenderError::ExceedMaxBaseFee
            })?;
        for (sig, calldata) in pending.into_iter() {
            // Start a database transaction
            let mut transaction = storage.start_transaction().await.unwrap();

            // Choose the appropriate client for TEE operations
            let sender_addr = self.eth_client_tee_dcap.as_ref().unwrap().sender_account();

            // Get the next nonce for the TEE transactions
            let nonce = self
                .get_next_nonce(&mut transaction, sender_addr, true)
                .await?;

            // Save the TEE transaction to the database
            let mut eth_tx = transaction
                .eth_sender_dal()
                .save_eth_tx(
                    nonce,
                    calldata,
                    AggregatedActionType::Tee,
                    self.eth_client_tee_dcap.as_ref().unwrap().contract_addr(),
                    // FIXME: TEE
                    Some(L1GasCriterion::total_tee_gas_amount()),
                    Some(sender_addr),
                    None, // No sidecar for TEE operations
                    false,
                )
                .await
                .unwrap();

            transaction
                .eth_sender_dal()
                .set_chain_id(eth_tx.id, self.sl_chain_id.0)
                .await
                .unwrap();
            eth_tx.chain_id = Some(self.sl_chain_id);

            transaction
                .tee_proof_generation_dal()
                .set_eth_tx_id_for_proof(&sig, eth_tx.id)
                .await
                .map_err(|_e| {
                    // FIXME: TEE
                    EthSenderError::ExceedMaxBaseFee
                })?;

            // Commit the transaction
            transaction.commit().await.unwrap();

            tracing::info!("eth_tx with ID {} for block proof", eth_tx.id);
        }
        Ok(())
    }

    // FIXME: TEE
    async fn aggregate_tee_dcap_transactions(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EthSenderError> {
        let pending = storage
            .tee_dcap_collateral_dal()
            .get_pending_collateral_for_eth_tx()
            .await
            .map_err(|_e| {
                // FIXME: TEE
                EthSenderError::ExceedMaxBaseFee
            })?;

        // Generate TEE data
        for pending_collateral in pending.into_iter() {
            // Start a database transaction
            let mut transaction = storage.start_transaction().await.unwrap();

            // Choose the appropriate client for TEE operations
            let sender_addr = self.eth_client_tee_dcap.as_ref().unwrap().sender_account();

            // Get the next nonce for the TEE transactions
            let nonce = self
                .get_next_nonce(&mut transaction, sender_addr, true)
                .await?;

            // Save the TEE transaction to the database
            let mut eth_tx = transaction
                .eth_sender_dal()
                .save_eth_tx(
                    nonce,
                    pending_collateral.calldata().to_vec(),
                    AggregatedActionType::Tee,
                    self.eth_client_tee_dcap.as_ref().unwrap().contract_addr(),
                    // FIXME: TEE
                    Some(L1GasCriterion::total_tee_gas_amount()),
                    Some(sender_addr),
                    None, // No sidecar for TEE operations
                    false,
                )
                .await
                .unwrap();

            transaction
                .eth_sender_dal()
                .set_chain_id(eth_tx.id, self.sl_chain_id.0)
                .await
                .unwrap();
            eth_tx.chain_id = Some(self.sl_chain_id);

            match pending_collateral {
                PendingCollateral::Field(PendingFieldCollateral { kind, .. }) => {
                    transaction
                        .tee_dcap_collateral_dal()
                        .set_field_eth_tx_id(kind, eth_tx.id as i32)
                        .await
                        .map_err(|_e| {
                            // FIXME: TEE
                            EthSenderError::ExceedMaxBaseFee
                        })?;
                }
                PendingCollateral::TcbInfo(PendingTcbInfoCollateral { kind, fmspc, .. }) => {
                    transaction
                        .tee_dcap_collateral_dal()
                        .set_tcb_info_eth_tx_id(kind, &fmspc, eth_tx.id as i32)
                        .await
                        .map_err(|_e| {
                            // FIXME: TEE
                            EthSenderError::ExceedMaxBaseFee
                        })?;
                }
            }

            // Commit the transaction
            transaction.commit().await.unwrap();

            tracing::info!("eth_tx with ID {} for op TEE", eth_tx.id);
        }

        Ok(())
    }

    // FIXME: TEE
    async fn aggregate_tee_attestations(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EthSenderError> {
        let pending = storage
            .tee_proof_generation_dal()
            .get_pending_attestations_for_eth_tx()
            .await
            .map_err(|_e| {
                // FIXME: TEE
                EthSenderError::ExceedMaxBaseFee
            })?;
        // Generate TEE data
        for (pubkey, calldata) in pending.into_iter() {
            // Start a database transaction
            let mut transaction = storage.start_transaction().await.unwrap();

            // Choose the appropriate client for TEE operations
            let sender_addr = self.eth_client_tee_dcap.as_ref().unwrap().sender_account();

            // Get the next nonce for the TEE transactions
            let nonce = self
                .get_next_nonce(&mut transaction, sender_addr, true)
                .await?;

            // Save the TEE transaction to the database
            let mut eth_tx = transaction
                .eth_sender_dal()
                .save_eth_tx(
                    nonce,
                    calldata,
                    AggregatedActionType::Tee,
                    self.eth_client_tee_dcap.as_ref().unwrap().contract_addr(),
                    // FIXME: TEE
                    Some(L1GasCriterion::total_tee_gas_amount()),
                    Some(sender_addr),
                    None, // No sidecar for TEE operations
                    false,
                )
                .await
                .unwrap();

            transaction
                .eth_sender_dal()
                .set_chain_id(eth_tx.id, self.sl_chain_id.0)
                .await
                .unwrap();
            eth_tx.chain_id = Some(self.sl_chain_id);

            transaction
                .tee_proof_generation_dal()
                .set_eth_tx_id_for_attestation(&pubkey, eth_tx.id)
                .await
                .map_err(|_e| {
                    // FIXME: TEE
                    EthSenderError::ExceedMaxBaseFee
                })?;

            // Commit the transaction
            transaction.commit().await.unwrap();

            tracing::info!(
                "eth_tx with ID {} for attestation for TEE pubkey {}",
                eth_tx.id,
                hex::encode(pubkey)
            );
        }

        Ok(())
    }

    async fn report_eth_tx_saving(
        storage: &mut Connection<'_, Core>,
        aggregated_op: &AggregatedOperation,
        tx: &EthTx,
    ) {
        let l1_batch_number_range = aggregated_op.l1_batch_range();
        tracing::info!(
            "eth_tx with ID {} for op {} was saved for L1 batches {l1_batch_number_range:?}",
            tx.id,
            aggregated_op.get_action_caption()
        );

        if let AggregatedOperation::Commit(_, l1_batches, _, _) = aggregated_op {
            for batch in l1_batches {
                METRICS.pubdata_size[&PubdataKind::StateDiffs]
                    .observe(batch.metadata.state_diffs_compressed.len());
                METRICS.pubdata_size[&PubdataKind::UserL2ToL1Logs]
                    .observe(batch.header.l2_to_l1_logs.len() * UserL2ToL1Log::SERIALIZED_SIZE);
                METRICS.pubdata_size[&PubdataKind::LongL2ToL1Messages]
                    .observe(batch.header.l2_to_l1_messages.iter().map(Vec::len).sum());
                METRICS.pubdata_size[&PubdataKind::RawPublishedBytecodes]
                    .observe(batch.raw_published_factory_deps.iter().map(Vec::len).sum());
            }
        }

        let range_size = l1_batch_number_range.end().0 - l1_batch_number_range.start().0 + 1;
        METRICS.block_range_size[&aggregated_op.get_action_type().into()]
            .observe(range_size.into());
        METRICS
            .track_eth_tx_metrics(storage, BlockL1Stage::Saved, tx)
            .await;
    }

    fn encode_aggregated_op(
        &self,
        op: &AggregatedOperation,
        chain_protocol_version_id: ProtocolVersionId,
    ) -> TxData {
        let mut args = vec![Token::Uint(self.rollup_chain_id.as_u64().into())];
        let is_op_pre_gateway = op.protocol_version().is_pre_gateway();

        let (calldata, sidecar) = match op {
            AggregatedOperation::Commit(
                last_committed_l1_batch,
                l1_batches,
                pubdata_da,
                commitment_mode,
            ) => {
                let commit_batches = CommitBatches {
                    last_committed_l1_batch,
                    l1_batches,
                    pubdata_da: *pubdata_da,
                    mode: *commitment_mode,
                };
                let commit_data_base = commit_batches.into_tokens();

                args.extend(commit_data_base);
                let commit_data = args;
                let encoding_fn = if is_op_pre_gateway {
                    &self.functions.post_shared_bridge_commit
                } else {
                    &self.functions.post_gateway_commit
                };

                let l1_batch_for_sidecar = if PubdataSendingMode::Blobs == *pubdata_da {
                    Some(l1_batches[0].clone())
                } else {
                    None
                };

                Self::encode_commit_data(encoding_fn, &commit_data, l1_batch_for_sidecar)
            }
            AggregatedOperation::PublishProofOnchain(op) => {
                args.extend(op.conditional_into_tokens(self.config.is_verifier_pre_fflonk));
                let encoding_fn = if is_op_pre_gateway {
                    &self.functions.post_shared_bridge_prove
                } else {
                    &self.functions.post_gateway_prove
                };
                let calldata = encoding_fn
                    .encode_input(&args)
                    .expect("Failed to encode prove transaction data");
                (calldata, None)
            }
            AggregatedOperation::Execute(op) => {
                args.extend(op.encode_for_eth_tx(chain_protocol_version_id));
                let encoding_fn = if is_op_pre_gateway && chain_protocol_version_id.is_pre_gateway()
                {
                    &self.functions.post_shared_bridge_execute
                } else {
                    &self.functions.post_gateway_execute
                };
                let calldata = encoding_fn
                    .encode_input(&args)
                    .expect("Failed to encode execute transaction data");
                (calldata, None)
            }
        };
        TxData { calldata, sidecar }
    }

    fn encode_commit_data(
        commit_fn: &Function,
        commit_payload: &[Token],
        l1_batch: Option<L1BatchWithMetadata>,
    ) -> (Vec<u8>, Option<EthTxBlobSidecar>) {
        let calldata = commit_fn
            .encode_input(commit_payload)
            .expect("Failed to encode commit transaction data");

        let sidecar = match l1_batch {
            None => None,
            Some(l1_batch) => {
                let sidecar = l1_batch
                    .header
                    .pubdata_input
                    .clone()
                    .unwrap()
                    .chunks(ZK_SYNC_BYTES_PER_BLOB)
                    .map(|blob| {
                        let kzg_info = KzgInfo::new(blob);
                        SidecarBlobV1 {
                            blob: kzg_info.blob.to_vec(),
                            commitment: kzg_info.kzg_commitment.to_vec(),
                            proof: kzg_info.blob_proof.to_vec(),
                            versioned_hash: kzg_info.versioned_hash.to_vec(),
                        }
                    })
                    .collect::<Vec<SidecarBlobV1>>();

                let eth_tx_blob_sidecar = EthTxBlobSidecarV1 { blobs: sidecar };
                Some(eth_tx_blob_sidecar.into())
            }
        };

        (calldata, sidecar)
    }

    pub(super) async fn save_eth_tx(
        &self,
        storage: &mut Connection<'_, Core>,
        aggregated_op: &AggregatedOperation,
        timelock_contract_address: Address,
        chain_protocol_version_id: ProtocolVersionId,
        is_gateway: bool,
    ) -> Result<EthTx, EthSenderError> {
        let mut transaction = storage.start_transaction().await.unwrap();
        let op_type = aggregated_op.get_action_type();
        // We may be using a custom sender for commit transactions, so use this
        // var whatever it actually is: a `None` for single-addr operator or `Some`
        // for multi-addr operator in 4844 mode.
        let (sender_addr, is_non_blob_sender) = match (op_type, is_gateway) {
            (AggregatedActionType::Commit, false) => self
                .eth_client_blobs
                .as_ref()
                .map(|c| (c.sender_account(), false))
                .unwrap_or_else(|| (self.eth_client.sender_account(), true)),
            (_, _) => (self.eth_client.sender_account(), true),
        };
        let nonce = self
            .get_next_nonce(&mut transaction, sender_addr, is_non_blob_sender)
            .await?;
        let encoded_aggregated_op =
            self.encode_aggregated_op(aggregated_op, chain_protocol_version_id);
        let l1_batch_number_range = aggregated_op.l1_batch_range();

        let eth_tx_predicted_gas = match op_type {
            AggregatedActionType::Execute => {
                L1GasCriterion::total_execute_gas_amount(
                    &mut transaction,
                    l1_batch_number_range.clone(),
                    is_gateway,
                )
                .await
            }
            AggregatedActionType::PublishProofOnchain => {
                L1GasCriterion::total_proof_gas_amount(is_gateway)
            }
            AggregatedActionType::Commit => L1GasCriterion::total_commit_validium_gas_amount(
                l1_batch_number_range.clone(),
                is_gateway,
            ),
            AggregatedActionType::Tee => {
                // FIXME: TEE
                L1GasCriterion::total_tee_gas_amount()
            }
        };

        let mut eth_tx = transaction
            .eth_sender_dal()
            .save_eth_tx(
                nonce,
                encoded_aggregated_op.calldata,
                op_type,
                timelock_contract_address,
                Some(eth_tx_predicted_gas),
                Some(sender_addr),
                encoded_aggregated_op.sidecar,
                is_gateway,
            )
            .await
            .unwrap();

        transaction
            .eth_sender_dal()
            .set_chain_id(eth_tx.id, self.sl_chain_id.0)
            .await
            .unwrap();
        eth_tx.chain_id = Some(self.sl_chain_id);
        transaction
            .blocks_dal()
            .set_eth_tx_id(l1_batch_number_range, eth_tx.id, op_type)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        Ok(eth_tx)
    }

    // Just because we block all operations during gateway migration,
    // this function should not be called when the settlement layer is unknown
    fn is_gateway(&self) -> bool {
        self.settlement_layer
            .as_ref()
            .map(|sl| sl.is_gateway())
            .unwrap_or(false)
    }

    async fn get_next_nonce(
        &self,
        storage: &mut Connection<'_, Core>,
        from_addr: Address,
        is_non_blob_sender: bool,
    ) -> Result<u64, EthSenderError> {
        let is_gateway = self.is_gateway();
        let db_nonce = storage
            .eth_sender_dal()
            .get_next_nonce(from_addr, is_non_blob_sender, is_gateway)
            .await
            .unwrap()
            .unwrap_or(0);
        // Between server starts we can execute some txs using operator account or remove some txs from the database
        // At the start we have to consider this fact and get the max nonce.
        let l1_nonce = self.initial_pending_nonces[&from_addr];
        tracing::info!(
            "Next nonce from db: {}, nonce from L1: {} for address: {:?}",
            db_nonce,
            l1_nonce,
            from_addr
        );
        Ok(db_nonce.max(l1_nonce))
    }

    /// Returns the health check for eth tx aggregator.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    async fn gateway_status(&self, storage: &mut Connection<'_, Core>) -> GatewayMigrationState {
        let notification = storage
            .server_notifications_dal()
            .get_latest_gateway_migration_notification()
            .await
            .unwrap();

        GatewayMigrationState::from_sl_and_notification(self.settlement_layer, notification)
    }
}

async fn query_contract(
    l1_client: &dyn BoundEthInterface,
    method_name: &str,
    params: &[Token],
) -> Result<Token, EthSenderError> {
    let data = l1_client
        .contract()
        .function(method_name)
        .unwrap()
        .encode_input(params)
        .unwrap();

    let eth_interface: &dyn EthInterface = AsRef::<dyn EthInterface>::as_ref(l1_client);

    let result = eth_interface
        .call_contract_function(
            CallRequest {
                data: Some(data.into()),
                to: Some(l1_client.contract_addr()),
                ..CallRequest::default()
            },
            None,
        )
        .await?;
    Ok(l1_client
        .contract()
        .function(method_name)
        .unwrap()
        .decode_output(&result.0)
        .unwrap()[0]
        .clone())
}

async fn get_priority_tree_start_index(
    l1_client: &dyn BoundEthInterface,
) -> Result<Option<usize>, EthSenderError> {
    let packed_semver = query_contract(l1_client, "getProtocolVersion", &[])
        .await?
        .into_uint()
        .unwrap();

    // We always expect the provided version to be correct, so we panic if it is not
    let version = ProtocolVersionId::try_from_packed_semver(packed_semver).unwrap();

    // For pre-gateway versions the index is not supported.
    if version.is_pre_gateway() {
        return Ok(None);
    }

    let priority_tree_start_index = query_contract(l1_client, "getPriorityTreeStartIndex", &[])
        .await?
        .into_uint()
        .unwrap();

    Ok(Some(priority_tree_start_index.as_usize()))
}
