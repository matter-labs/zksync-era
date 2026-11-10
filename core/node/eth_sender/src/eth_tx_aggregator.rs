use std::{collections::HashMap, time::Duration};

use tokio::sync::watch;
use zksync_config::configs::eth_sender::{PrecommitParams, ProverType, SenderConfig};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{
    convert_eip4844_sidecar_to_eip7594_sidecar, BoundEthInterface, CallFunctionArgs,
    ContractCallError, EthInterface,
};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_l1_contract_interface::{
    i_executor::{
        commit::kzg::{KzgInfo, ZK_SYNC_BYTES_PER_BLOB},
        methods::{CommitBatches, PrecommitBatches},
    },
    multicall3::{Multicall3Call, Multicall3Result},
    Tokenizable, Tokenize,
};
use zksync_shared_metrics::L1Stage;
use zksync_types::{
    aggregated_operations::{
        AggregatedActionType, L1BatchAggregatedActionType, L2BlockAggregatedActionType,
    },
    commitment::{L1BatchWithMetadata, L2DACommitmentScheme, SerializeCommitment},
    eth_sender::{EthTx, EthTxBlobSidecar, EthTxBlobSidecarV1, EthTxBlobSidecarV2, SidecarBlobV1},
    ethabi::{Function, Param, ParamType, StateMutability, Token},
    l2_to_l1_log::UserL2ToL1Log,
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion, PACKED_SEMVER_MINOR_MASK},
    pubdata_da::PubdataSendingMode,
    server_notification::GatewayMigrationState,
    settlement::SettlementLayer,
    web3::{contract::Error as Web3ContractError, BlockId, BlockNumber, CallRequest},
    Address, L2ChainId, ProtocolVersionId, SLChainId, H256, U256,
};

mod protocol_upgrade;

use super::aggregated_operations::{
    AggregatedOperation, L1BatchAggregatedOperation, L2BlockAggregatedOperation,
};
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
    pub l1_validator: Address,
    pub l2_da_commitment_scheme: Option<L2DACommitmentScheme>,
    pub l2_validator: Option<Address>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EthereumUpgradeState {
    NotStarted,
    Pending,
    Finished,
}

/// Data queried from L1 using multicall contract.
#[derive(Debug)]
#[allow(dead_code)]
pub struct MulticallData {
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub verifier_address: Address,
    pub chain_protocol_version: ProtocolSemanticVersion,
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
    pool: ConnectionPool<Core>,
    sl_chain_id: SLChainId,
    health_updater: HealthUpdater,
    priority_tree_start_index: Option<usize>,
    settlement_layer: Option<SettlementLayer>,
    initial_pending_nonces: HashMap<Address, u64>,
    needs_to_check_precommit: bool,
}

struct TxData {
    calldata: Vec<u8>,
    sidecar: Option<EthTxBlobSidecar>,
}

const FFLONK_VERIFIER_TYPE: i32 = 0;
/// Airbender key-discovery index on EraMultiProofVerifier.
const AIRBENDER_PLONK_VERIFIER_TYPE: i32 = 2;

#[derive(Debug, PartialEq)]
struct DisabledProofSystems(u8);

impl zksync_l1_contract_interface::Detokenize for DisabledProofSystems {
    fn from_tokens(tokens: Vec<Token>) -> Result<Self, zksync_types::web3::contract::Error> {
        if let [Token::Tuple(fields)] = tokens.as_slice() {
            if let [Token::Bool(boojum), Token::Bool(airbender)] = fields.as_slice() {
                return Ok(Self(u8::from(*boojum) | (u8::from(*airbender) << 1)));
            }
        }
        Err(zksync_types::web3::contract::Error::InvalidOutputType(
            "expected DisabledProofSystems(bool,bool)".into(),
        ))
    }
}

async fn proof_systems_match_notification(
    storage: &mut Connection<'_, Core>,
    current_mask: u8,
) -> Result<bool, EthSenderError> {
    if let Some(expected) = storage
        .server_notifications_dal()
        .get_latest_proof_system_status_notification()
        .await?
    {
        if expected.disabled_proof_systems != current_mask {
            tracing::info!(
                expected = expected.disabled_proof_systems,
                current = current_mask,
                "Waiting for announced proof system status"
            );
            return Ok(false);
        }
    }
    Ok(true)
}

fn resolve_prover_policy(
    configured_prover: ProverType,
    mask: U256,
    supported: U256,
    required: U256,
    proof_type: U256,
) -> Result<ProverType, String> {
    if mask > U256::from(2)
        || proof_type != U256::from(4)
        || supported.is_zero()
        || supported > U256::from(3)
        || required.is_zero()
        || required != (supported & (U256::from(3) ^ mask))
    {
        return Err("unsupported verifier policy".into());
    }
    let configured = U256::from(3 ^ configured_prover.disabled_proof_systems());
    if (configured & required) != required {
        return Err(format!(
            "configured {configured_prover:?} does not supply required systems {required}"
        ));
    }
    ProverType::from_disabled_proof_systems(3 ^ required.as_u32() as u8)
        .map_err(|err| err.to_string())
}

#[allow(deprecated)]
fn view_function(name: &str, inputs: Vec<ParamType>, output: ParamType) -> Function {
    Function {
        name: name.into(),
        inputs: inputs
            .into_iter()
            .map(|kind| Param {
                name: String::new(),
                kind,
                internal_type: None,
            })
            .collect(),
        outputs: vec![Param {
            name: String::new(),
            kind: output,
            internal_type: None,
        }],
        constant: None,
        state_mutability: StateMutability::View,
    }
}

impl EthTxAggregator {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: ConnectionPool<Core>,
        config: SenderConfig,
        aggregator: Aggregator,
        eth_client: Box<dyn BoundEthInterface>,
        eth_client_blobs: Option<Box<dyn BoundEthInterface>>,
        config_timelock_contract_address: Address,
        state_transition_manager_address: Address,
        l1_multicall3_address: Address,
        state_transition_chain_contract: Address,
        rollup_chain_id: L2ChainId,
        settlement_layer: Option<SettlementLayer>,
    ) -> Self {
        let eth_client = eth_client.for_component("eth_tx_aggregator");
        let eth_client_blobs = eth_client_blobs.map(|c| c.for_component("eth_tx_aggregator"));

        let functions = ZkSyncFunctions::default();

        let mut initial_pending_nonces = HashMap::new();
        for client in eth_client_blobs.iter().chain(std::iter::once(&eth_client)) {
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
            pool,
            sl_chain_id,
            health_updater: ReactiveHealthCheck::new("eth_tx_aggregator").1,
            priority_tree_start_index: None,
            settlement_layer,
            initial_pending_nonces,
            needs_to_check_precommit: true,
        }
    }

    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.health_updater
            .update(Health::from(HealthStatus::Ready));

        tracing::info!(
            "Initialized eth_tx_aggregator with is_pre_fflonk_verifier: {:?}",
            self.config.is_verifier_pre_fflonk
        );

        let pool = self.pool.clone();
        while !*stop_receiver.borrow() {
            let mut storage = pool.connection_tagged("eth_sender").await.unwrap();
            if let Err(err) = self.loop_iteration(&mut storage).await {
                // Web3 API request failures can cause this,
                // and anything more important is already properly reported.
                tracing::warn!("eth_sender error {err:?}");
            }
            drop(storage);

            // The stop receiver status will be checked immediately in the loop condition.
            tokio::time::timeout(
                self.config.aggregate_tx_poll_period,
                stop_receiver.changed(),
            )
            .await
            .ok();
        }

        tracing::info!("Stop request received, eth_tx_aggregator is shutting down");
        Ok(())
    }

    pub(super) async fn fusaka_activation_state(
        &self,
    ) -> Result<EthereumUpgradeState, EthSenderError> {
        if self.config.fusaka_upgrade_block == Some(0) {
            return Ok(EthereumUpgradeState::Finished);
        }

        if self.config.fusaka_upgrade_timestamp.is_none()
            && self.config.fusaka_upgrade_block.is_none()
        {
            return Ok(EthereumUpgradeState::NotStarted);
        }

        let current_block = self
            .eth_client
            .block(BlockId::Number(BlockNumber::Latest))
            .await?
            .expect("Latest block not found");

        // Prioritize using the block number for the upgrade if both are set
        // Timestamp is set with default, so block number takes precedence
        match (
            self.config.fusaka_upgrade_block,
            self.config.fusaka_upgrade_timestamp,
        ) {
            (Some(fusaka_upgrade_block), _) => {
                if current_block.number.unwrap().as_u64() - self.config.fusaka_upgrade_safety_margin
                    < fusaka_upgrade_block
                {
                    Ok(EthereumUpgradeState::NotStarted)
                } else if current_block.number.unwrap().as_u64()
                    + self.config.fusaka_upgrade_safety_margin
                    >= fusaka_upgrade_block
                {
                    Ok(EthereumUpgradeState::Finished)
                } else {
                    Ok(EthereumUpgradeState::Pending)
                }
            }
            (_, Some(fusaka_upgrade_timestamp)) => {
                let current_timestamp = current_block.timestamp.as_u64();
                if current_timestamp
                    < fusaka_upgrade_timestamp - self.config.fusaka_upgrade_safety_margin
                {
                    Ok(EthereumUpgradeState::NotStarted)
                } else if current_timestamp + self.config.fusaka_upgrade_safety_margin
                    >= fusaka_upgrade_timestamp
                {
                    Ok(EthereumUpgradeState::Finished)
                } else {
                    Ok(EthereumUpgradeState::Pending)
                }
            }
            // All the values has already been checked this case is rather unreachable.
            // But for safety reasons, it's better to not panic if it's not necessary
            (_, _) => Ok(EthereumUpgradeState::NotStarted),
        }
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

        let get_stm_pre_v29_validator_timelock_input = self
            .functions
            .state_transition_manager_contract
            .function("validatorTimelock")
            .unwrap()
            .encode_input(&[])
            .unwrap();
        let get_stm_pre_v29_validator_timelock_call = Multicall3Call {
            target: self.state_transition_manager_address,
            allow_failure: ALLOW_FAILURE,
            calldata: get_stm_pre_v29_validator_timelock_input,
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

        let get_post_v29_upgradeable_validator_timelock_input = self
            .functions
            .state_transition_manager_contract
            .function("validatorTimelockPostV29")
            .unwrap()
            .encode_input(&[])
            .unwrap();

        let get_post_v29_upgradeable_validator_timelock_call = Multicall3Call {
            target: self.state_transition_manager_address,
            // Note, that this call is allowed to fail, as the corresponding function is not present in the pre-v29 protocol versions
            allow_failure: true,
            calldata: get_post_v29_upgradeable_validator_timelock_input,
        };

        let mut token_vec = vec![
            get_bootloader_hash_call.into_token(),
            get_default_aa_hash_call.into_token(),
            get_verifier_params_call.into_token(),
            get_verifier_call.into_token(),
            get_protocol_version_call.into_token(),
            get_stm_protocol_version_call.into_token(),
            get_stm_pre_v29_validator_timelock_call.into_token(),
            get_da_validator_pair_call.into_token(),
            get_post_v29_upgradeable_validator_timelock_call.into_token(),
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
            let number_of_calls = if evm_emulator_hash_requested { 10 } else { 9 };
            // 9 or 10 calls are aggregated in multicall (added post-v29 validator timelock call)
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

            let chain_protocol_version = Self::parse_protocol_semver(
                call_results_iterator.next().unwrap(),
                "contract protocol version",
            )?;
            let chain_protocol_version_id = chain_protocol_version.minor;
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
                chain_protocol_version_id,
            )?;

            let stm_validator_timelock_address =
                if chain_protocol_version_id.is_pre_interop_fast_blocks() {
                    // We just skip the result for the pre-V29 upgradeable validator timelock
                    call_results_iterator.next().unwrap();

                    stm_validator_timelock_address
                } else {
                    Self::parse_address(
                        call_results_iterator.next().unwrap(),
                        "post-V29 upgradeable validator timelock",
                    )?
                };

            return Ok(MulticallData {
                base_system_contracts_hashes,
                verifier_address,
                chain_protocol_version,
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
        Ok(Self::parse_protocol_semver(data, name)?.minor)
    }

    fn parse_protocol_semver(
        data: Token,
        name: &'static str,
    ) -> Result<ProtocolSemanticVersion, EthSenderError> {
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
        let protocol_version = if protocol_version < U256::from(PACKED_SEMVER_MINOR_MASK) {
            ProtocolSemanticVersion {
                minor: ProtocolVersionId::try_from(protocol_version.as_u32() as u16).unwrap(),
                patch: 0.into(),
            }
        } else {
            ProtocolSemanticVersion::try_from_packed(protocol_version).unwrap()
        };

        Ok(protocol_version)
    }

    fn parse_address(data: Token, name: &'static str) -> Result<Address, EthSenderError> {
        let result = Multicall3Result::from_token(data)?;
        if !result.success {
            return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                format!("multicall3 {name} call failed"),
            )));
        }
        let multicall_data = result.return_data;
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
        protocol_version_id: ProtocolVersionId,
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

        let l1_validator = Address::from_slice(&multicall_data[L1_DA_VALIDATOR_OFFSET..32]);

        let pair = if protocol_version_id.is_pre_medium_interop() {
            DAValidatorPair {
                l1_validator,
                l2_validator: Some(Address::from_slice(
                    &multicall_data[L2_DA_VALIDATOR_OFFSET..64],
                )),
                l2_da_commitment_scheme: None,
            }
        } else {
            let raw_l2_da_commitment_scheme =
                U256::from_big_endian(&multicall_data[L2_DA_VALIDATOR_OFFSET..64]);
            if raw_l2_da_commitment_scheme > U256::from(u8::MAX) {
                return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                    format!(
                        "Invalid L2DACommitmentScheme value in {name}: {}",
                        raw_l2_da_commitment_scheme
                    ),
                )));
            }

            DAValidatorPair {
                l1_validator,
                l2_da_commitment_scheme: Some(
                    L2DACommitmentScheme::try_from(raw_l2_da_commitment_scheme.as_u64() as u8)
                        .map_err(|_| {
                            EthSenderError::Parse(Web3ContractError::InvalidOutputType(format!(
                                "Unsupported L2DACommitmentScheme value in {name}: {}",
                                raw_l2_da_commitment_scheme
                            )))
                        })?,
                ),
                l2_validator: None,
            }
        };
        Ok(pair)
    }

    /// Returns `None` if the getter call itself has failed, which is the case when it is not
    /// present on the deployed timelock.
    fn parse_execution_delay(
        data: Token,
        name: &'static str,
    ) -> Result<Option<Duration>, EthSenderError> {
        let multicall_data = Multicall3Result::from_token(data)?;

        if !multicall_data.success {
            tracing::warn!(
                "multicall3 {name} call has failed: {:?}, ignoring the returned delay",
                multicall_data.return_data
            );
            return Ok(None);
        }

        let delay_seconds = U256::from_big_endian(&multicall_data.return_data);
        Ok(Some(Duration::from_secs(delay_seconds.as_u64())))
    }

    /// Reads the execution delay enforced for this chain from `timelock_address`, which has to be
    /// the timelock the execute transactions are sent to, as returned by
    /// [`Self::timelock_contract_address`]. The delay is stored per chain, so reading it from any
    /// other timelock would silently yield the ecosystem-wide value.
    ///
    /// Both getters are allowed to fail: `getExecutionDelay` is only present starting from v29
    /// timelocks, and some setups have no timelock at all. A reverting call is ignored rather than
    /// counted as a zero delay, while a transport error still surfaces as an error.
    pub(super) async fn get_execution_delay(
        &self,
        timelock_address: Address,
    ) -> Result<Duration, EthSenderError> {
        const ALLOW_FAILURE: bool = true;
        let timelock_contract = &self.functions.validator_timelock_contract;

        // The ecosystem-wide floor, present on every version of the timelock.
        let ecosystem_delay_call = Multicall3Call {
            target: timelock_address,
            allow_failure: ALLOW_FAILURE,
            calldata: timelock_contract
                .function("executionDelay")
                .unwrap()
                .encode_input(&[])
                .unwrap(),
        };
        // The delay enforced for this chain in `ValidatorTimelock.executeBatchesSharedBridge`,
        // i.e. `max(executionDelay, chainExecutionDelay[chain])`. A chain admin can raise its own
        // delay above the ecosystem-wide one, in which case scheduling execute transactions based
        // on `executionDelay` alone would make them revert with `TimeNotReached`.
        let chain_delay_call = Multicall3Call {
            target: timelock_address,
            allow_failure: ALLOW_FAILURE,
            calldata: timelock_contract
                .function("getExecutionDelay")
                .unwrap()
                .encode_input(&[Token::Address(self.state_transition_chain_contract)])
                .unwrap(),
        };

        let args = CallFunctionArgs::new(
            &self.functions.aggregate3.name,
            vec![
                ecosystem_delay_call.into_token(),
                chain_delay_call.into_token(),
            ],
        )
        .for_contract(
            self.l1_multicall3_address,
            &self.functions.multicall_contract,
        );
        let aggregate3_result: Token = args.call((*self.eth_client).as_ref()).await?;

        let Token::Array(call_results) = aggregate3_result else {
            return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                format!("Failed to parse execution delay multicall token: {aggregate3_result:?}"),
            )));
        };
        let [ecosystem_delay_result, chain_delay_result] = <[Token; 2]>::try_from(call_results)
            .map_err(|results| {
                EthSenderError::Parse(Web3ContractError::InvalidOutputType(format!(
                    "Expected 2 execution delay calls in multicall, got: {results:?}"
                )))
            })?;

        let ecosystem_delay =
            Self::parse_execution_delay(ecosystem_delay_result, "ecosystem-wide execution delay")?;
        let chain_delay =
            Self::parse_execution_delay(chain_delay_result, "chain-specific execution delay")?;
        // `getExecutionDelay` already returns the maximum of the two delays, but it is missing on
        // pre-v29 timelocks; taking the maximum here keeps the ecosystem-wide value as a floor
        // regardless of which of the two getters is available.
        Ok(chain_delay
            .unwrap_or_default()
            .max(ecosystem_delay.unwrap_or_default()))
    }

    pub(super) fn timelock_contract_address(
        &self,
        chain_protocol_version_id: ProtocolVersionId,
        stm_protocol_version_id: ProtocolVersionId,
        stm_validator_timelock_address: Address,
    ) -> Address {
        // For chains before v26 (gateway) we use the timelock address from config.
        // After that, the timelock address can be fetched from STM as it is the valid one
        // for versions starting from v26 and is not expected to change in the near future.
        if chain_protocol_version_id < ProtocolVersionId::gateway_upgrade()
            || self.config.force_use_validator_timelock
        {
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
            .call_with_function((*self.eth_client).as_ref(), get_vk_hash.clone())
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
        let function = self
            .functions
            .verifier_contract
            .functions_by_name(&get_vk_hash.name)
            .map_err(|x| EthSenderError::ContractCall(ContractCallError::Function(x)))?
            .iter()
            .find(|function| function.inputs.len() == 1);

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

    async fn sync_proof_systems(
        &mut self,
        storage: &mut Connection<'_, Core>,
        verifier: Address,
    ) -> Result<Option<&'static str>, EthSenderError> {
        let disabled: DisabledProofSystems = CallFunctionArgs::new("disabledProofSystems", ())
            .for_contract(
                self.state_transition_chain_contract,
                &self.functions.verifier_contract,
            )
            .call_with_function(
                (*self.eth_client).as_ref(),
                view_function(
                    "disabledProofSystems",
                    vec![],
                    ParamType::Tuple(vec![ParamType::Bool, ParamType::Bool]),
                ),
            )
            .await?;
        if !proof_systems_match_notification(storage, disabled.0).await? {
            return Ok(Some("waiting for announced proof system status"));
        }
        let mask = U256::from(disabled.0);
        let supported: U256 = CallFunctionArgs::new("supportedProofSystems", ())
            .for_contract(verifier, &self.functions.verifier_contract)
            .call_with_function(
                (*self.eth_client).as_ref(),
                view_function("supportedProofSystems", vec![], ParamType::Uint(8)),
            )
            .await?;
        let required: U256 = CallFunctionArgs::new("requiredProofSystems", mask)
            .for_contract(verifier, &self.functions.verifier_contract)
            .call_with_function(
                (*self.eth_client).as_ref(),
                view_function(
                    "requiredProofSystems",
                    vec![ParamType::Uint(8)],
                    ParamType::Uint(8),
                ),
            )
            .await?;
        let proof_type: U256 = CallFunctionArgs::new("acceptedProofType", ())
            .for_contract(verifier, &self.functions.verifier_contract)
            .call_with_function(
                (*self.eth_client).as_ref(),
                view_function("acceptedProofType", vec![], ParamType::Uint(256)),
            )
            .await?;
        match resolve_prover_policy(self.config.prover, mask, supported, required, proof_type) {
            Ok(prover) => {
                self.aggregator.set_prover(prover);
                Ok(None)
            }
            Err(reason) => {
                tracing::warn!(
                    %reason, %mask, %supported, %required, %proof_type,
                    configured_prover = ?self.config.prover,
                    "Proof submission paused: incompatible proof system policy"
                );
                Ok(Some("incompatible proof system policy"))
            }
        }
    }

    async fn get_airbender_snark_wrapper_vk_hash(
        &mut self,
        verifier_address: Address,
    ) -> Result<Option<H256>, EthSenderError> {
        let get_vk_hash = &self.functions.verification_key_hash;
        // Older verifiers without the Airbender key return `None`.
        let function = self
            .functions
            .verifier_contract
            .functions_by_name(&get_vk_hash.name)
            .map_err(|x| EthSenderError::ContractCall(ContractCallError::Function(x)))?
            .iter()
            .find(|function| function.inputs.len() == 1);

        if let Some(function) = function {
            let vk_hash: Option<H256> =
                CallFunctionArgs::new(&get_vk_hash.name, U256::from(AIRBENDER_PLONK_VERIFIER_TYPE))
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
            chain_protocol_version,
            chain_protocol_version_id,
            stm_protocol_version_id,
            stm_validator_timelock_address,
            da_validator_pair,
        } = self.get_multicall_data().await.map_err(|err| {
            tracing::error!("Failed to get multicall data {err:?}");
            err
        })?;

        // The timelock the execute transactions are sent to, and hence the one whose execution
        // delay is enforced for this chain.
        let timelock_contract_address = self.timelock_contract_address(
            chain_protocol_version_id,
            stm_protocol_version_id,
            stm_validator_timelock_address,
        );
        let execution_delay = self
            .get_execution_delay(timelock_contract_address)
            .await
            .map_err(|err| {
                tracing::error!("Failed to get execution delay from the ValidatorTimelock {err:?}");
                err
            })?
            .max(
                self.config
                    .l1_batch_min_age_before_execute
                    .unwrap_or_default(),
            );

        let fusaka_activation_state = self.fusaka_activation_state().await?;

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
        let airbender_snark_wrapper_vk_hash = self
            .get_airbender_snark_wrapper_vk_hash(verifier_address)
            .await
            .map_err(|err| {
                tracing::error!("Failed to get Airbender VK hash from the Verifier {err:?}");
                err
            })?;

        let prove_restriction = if chain_protocol_version_id >= ProtocolVersionId::Version34 {
            self.sync_proof_systems(storage, verifier_address).await?
        } else {
            None
        };

        let l1_verifier_config = L1VerifierConfig {
            snark_wrapper_vk_hash,
            fflonk_snark_wrapper_vk_hash,
            airbender_snark_wrapper_vk_hash,
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
            prove_restriction,
            execute_restriction: Self::is_pending_gateway_upgrade(
                storage,
                chain_protocol_version_id,
            )
            .await
            .then_some("there is a pending gateway upgrade"),
            // For precommit operations, we could safely use the commit restriction.
            precommit_restriction: commit_restriction,
        };

        if chain_protocol_version_id.is_pre_medium_interop() {
            if da_validator_pair.l1_validator == Address::zero()
                || da_validator_pair.l2_validator == Some(Address::zero())
            {
                let reason = Some("DA validator pair is not set on the settlement layer");
                op_restrictions.commit_restriction = reason;
                // We only disable commit operations, the rest are allowed
            }
        } else if da_validator_pair.l1_validator == Address::zero()
            || da_validator_pair.l2_da_commitment_scheme == Some(L2DACommitmentScheme::None)
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
            op_restrictions.precommit_restriction = reason;
        }

        let is_gateway = self.is_gateway_for_sending_txs(storage).await?;

        if gateway_migration_state == GatewayMigrationState::InProgress {
            let reason = Some("Gateway migration started");
            op_restrictions.commit_restriction = reason;
            op_restrictions.precommit_restriction = reason;
            // From V31 when migrating to or from gateway, we need to wait for all blocks to be executed,
            // so there is no restriction for prove and execute operations
            if self
                .is_waiting_for_batches_with_current_settlement_layer_to_be_committed(storage)
                .await?
            {
                // While old-settlement-layer batches are still uncommitted, keep
                // commits/precommits flowing so migration can finish draining them.
                op_restrictions.commit_restriction = None;
                op_restrictions.precommit_restriction = None;
            }
        }

        let precommit_params = self
            .precommit_params(storage, chain_protocol_version_id)
            .await?;

        if fusaka_activation_state == EthereumUpgradeState::Pending {
            op_restrictions.commit_restriction = Some("Fusaka upgrade is pending");
        }
        let use_fusaka_blob_format = fusaka_activation_state == EthereumUpgradeState::Finished;

        if self
            .handle_protocol_upgrade(
                storage,
                chain_protocol_version,
                timelock_contract_address,
                is_gateway,
                &mut op_restrictions,
            )
            .await?
        {
            return Ok(());
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
                precommit_params.as_ref(),
                execution_delay,
                is_gateway,
            )
            .await?
        {
            let tx = self
                .save_eth_tx(
                    storage,
                    &agg_op,
                    timelock_contract_address,
                    chain_protocol_version_id,
                    is_gateway,
                    use_fusaka_blob_format,
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

        if precommit_params.is_some() {
            // If we are using precommit operations,
            // we need to set the final precommit operation for l1 batches
            self.set_final_precommit_operation(storage).await?;
        }
        Ok(())
    }

    /// If we need to disable precommit operations, we can't do it straight away,
    /// we have to fully precommit the last batch with precommit txs.
    /// But we only need it for one batch, so after this one batch we have to return to execution without precommit operations.
    async fn precommit_params(
        &mut self,
        storage: &mut Connection<'_, Core>,
        chain_protocol_version_id: ProtocolVersionId,
    ) -> Result<Option<PrecommitParams>, EthSenderError> {
        if chain_protocol_version_id.is_pre_interop_fast_blocks() {
            // If we are in the pre-interop fast blocks mode, we don't use precommit operations
            return Ok(None);
        }
        if let Some(params) = self.config.precommit_params.clone() {
            return Ok(Some(params));
        }

        if !self.needs_to_check_precommit {
            return Ok(None);
        }

        let Some(last_committed) = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_committed_on_eth()
            .await?
        else {
            return Ok(None);
        };

        let needs_to_precommit = storage
            .blocks_dal()
            .any_precommit_txs_after_batch(last_committed)
            .await?;

        if needs_to_precommit {
            Ok(Some(PrecommitParams::fast_precommit()))
        } else {
            self.needs_to_check_precommit = false;
            Ok(None)
        }
    }

    /// The server is doing precommits based on the txs.
    /// That means, that the last fictive l2 block will never be included into precommit txs and it's the only way how the miniblock will have no txs.
    /// So we have to check if the last 2 rolling txs hashes for miniblocks are equal and if yes, we can set the final precommit tx id for the batch,
    /// and that will mean we can commit the batch.
    async fn set_final_precommit_operation(
        &mut self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EthSenderError> {
        let l2_blocks_by_batch = storage
            .blocks_dal()
            .get_last_l2_block_rolling_txs_hashes_by_batches()
            .await?;
        for (batch_number, l2_block) in l2_blocks_by_batch {
            if l2_block.len() != 2 {
                // We expect exactly 2 miniblocks for each batch. If not we will wait for the next iteration
                continue;
            }
            // l2_blocks[0] is the newest, l2_blocks[1] is previous (because of DESC order)
            if l2_block[0].rolling_txs_hash == l2_block[1].rolling_txs_hash {
                if let Some(eth_tx_id) = l2_block[1].precommit_eth_tx_id {
                    storage
                        .blocks_dal()
                        .set_eth_tx_id_for_l1_batches(
                            batch_number..=batch_number,
                            eth_tx_id as u32,
                            AggregatedActionType::L2Block(L2BlockAggregatedActionType::Precommit),
                        )
                        .await?
                }
            }
        }
        Ok(())
    }

    async fn report_eth_tx_saving(
        storage: &mut Connection<'_, Core>,
        aggregated_op: &AggregatedOperation,
        tx: &EthTx,
    ) {
        match aggregated_op {
            AggregatedOperation::L1Batch(aggregated_op) => {
                let l1_batch_number_range = aggregated_op.l1_batch_range();
                tracing::info!(
                    "eth_tx with ID {} for op {} was saved for L1 batches {l1_batch_number_range:?}",
                    tx.id,
                    aggregated_op.get_action_caption()
                );

                if let L1BatchAggregatedOperation::Commit(_, l1_batches, _, _) = aggregated_op {
                    for batch in l1_batches {
                        METRICS.pubdata_size[&PubdataKind::StateDiffs]
                            .observe(batch.metadata.state_diffs_compressed.len());
                        METRICS.pubdata_size[&PubdataKind::UserL2ToL1Logs].observe(
                            batch.header.l2_to_l1_logs.len() * UserL2ToL1Log::SERIALIZED_SIZE,
                        );
                        METRICS.pubdata_size[&PubdataKind::LongL2ToL1Messages]
                            .observe(batch.header.l2_to_l1_messages.iter().map(Vec::len).sum());
                        METRICS.pubdata_size[&PubdataKind::RawPublishedBytecodes]
                            .observe(batch.raw_published_factory_deps.iter().map(Vec::len).sum());
                    }
                }

                let range_size =
                    l1_batch_number_range.end().0 - l1_batch_number_range.start().0 + 1;
                METRICS.block_range_size[&aggregated_op.get_action_type().into()]
                    .observe(range_size.into());
                METRICS
                    .track_eth_tx_metrics(storage, L1Stage::Saved, tx)
                    .await;
            }
            AggregatedOperation::L2Block(op) => {
                let l2_block_number_range = op.l2_blocks_range();
                tracing::info!(
                    "eth_tx with ID {} for op {} was saved for L2 block {l2_block_number_range:?}",
                    tx.id,
                    op.get_action_caption(),
                );

                let range_size =
                    l2_block_number_range.end().0 - l2_block_number_range.start().0 + 1;
                METRICS.l2_blocks_range_size[&op.get_action_type().into()]
                    .observe(range_size.into());
            }
        }
    }

    fn encode_aggregated_op(
        &self,
        op: &AggregatedOperation,
        chain_protocol_version_id: ProtocolVersionId,
        use_fusaka_blob_format: bool,
    ) -> TxData {
        match op {
            AggregatedOperation::L1Batch(op) => {
                let protocol_version = op.protocol_version();

                let mut args = if protocol_version.is_pre_interop_fast_blocks() {
                    vec![Token::Uint(self.rollup_chain_id.as_u64().into())]
                } else {
                    vec![Token::Address(self.state_transition_chain_contract)]
                };

                let (calldata, sidecar) = match op {
                    L1BatchAggregatedOperation::Commit(
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
                        let encoding_fn = if protocol_version.is_pre_gateway() {
                            &self.functions.post_shared_bridge_commit
                        } else if protocol_version.is_pre_interop_fast_blocks() {
                            &self.functions.post_v26_gateway_commit
                        } else {
                            &self.functions.post_v29_interop_commit
                        };

                        let l1_batch_for_sidecar = if PubdataSendingMode::Blobs == *pubdata_da {
                            Some(l1_batches[0].clone())
                        } else {
                            None
                        };

                        Self::encode_commit_data(
                            encoding_fn,
                            &commit_data,
                            l1_batch_for_sidecar,
                            use_fusaka_blob_format,
                        )
                    }
                    L1BatchAggregatedOperation::PublishProofOnchain(op) => {
                        args.extend(op.conditional_into_tokens(self.config.is_verifier_pre_fflonk));
                        let encoding_fn = if protocol_version.is_pre_gateway() {
                            &self.functions.post_shared_bridge_prove
                        } else if protocol_version.is_pre_interop_fast_blocks() {
                            &self.functions.post_v26_gateway_prove
                        } else {
                            &self.functions.post_v29_timelock_interop_prove
                        };
                        let calldata = encoding_fn
                            .encode_input(&args)
                            .expect("Failed to encode prove transaction data");
                        (calldata, None)
                    }
                    L1BatchAggregatedOperation::Execute(op) => {
                        let settlement_fee_payer =
                            self.config.settlement_fee_payer.unwrap_or(Address::zero());
                        args.extend(
                            op.encode_for_eth_tx(chain_protocol_version_id, settlement_fee_payer),
                        );
                        let encoding_fn = if protocol_version.is_pre_gateway()
                            && chain_protocol_version_id.is_pre_gateway()
                        {
                            &self.functions.post_shared_bridge_execute
                        } else if chain_protocol_version_id.is_pre_interop_fast_blocks() {
                            &self.functions.post_v26_gateway_execute
                        } else {
                            &self.functions.post_v29_interop_execute
                        };

                        let calldata = encoding_fn
                            .encode_input(&args)
                            .expect("Failed to encode execute transaction data");
                        (calldata, None)
                    }
                };
                TxData { calldata, sidecar }
            }
            AggregatedOperation::L2Block(op) => match op {
                L2BlockAggregatedOperation::Precommit {
                    l1_batch: l1_batch_number,
                    last_l2_block,
                    txs,
                    ..
                } => {
                    let mut args = vec![Token::Address(self.state_transition_chain_contract)];

                    let precommit_batches = PrecommitBatches {
                        txs,
                        last_l2_block: *last_l2_block,
                        l1_batch_number: *l1_batch_number,
                    };
                    let precommit_data_base = precommit_batches.into_tokens();

                    args.extend(precommit_data_base);
                    let encoding_fn = &self.functions.post_v29_interop_precommit;

                    let calldata = encoding_fn
                        .encode_input(&args)
                        .expect("Failed to encode execute transaction data");
                    TxData {
                        calldata,
                        sidecar: None,
                    }
                }
            },
        }
    }

    fn encode_commit_data(
        commit_fn: &Function,
        commit_payload: &[Token],
        l1_batch: Option<L1BatchWithMetadata>,
        use_eip7594_blobs: bool,
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

                let eth_tx_blob_sidecar = if use_eip7594_blobs {
                    EthTxBlobSidecarV2 {
                        blobs: sidecar
                            .into_iter()
                            .map(|sidecar_blob| {
                                convert_eip4844_sidecar_to_eip7594_sidecar(sidecar_blob)
                            })
                            .collect(),
                    }
                    .into()
                } else {
                    EthTxBlobSidecarV1 { blobs: sidecar }.into()
                };
                Some(eth_tx_blob_sidecar)
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
        use_fusaka_blob_format: bool,
    ) -> Result<EthTx, EthSenderError> {
        let mut transaction = storage.start_transaction().await.unwrap();
        let op_type = aggregated_op.get_action_type();
        // We may be using a custom sender for commit transactions, so use this
        // var whatever it actually is: a `None` for single-addr operator or `Some`
        // for multi-addr operator in 4844 mode.
        let sender_addr = match (op_type, is_gateway) {
            (AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Commit), false) => self
                .eth_client_blobs
                .as_ref()
                .map(|c| c.sender_account())
                .unwrap_or_else(|| self.eth_client.sender_account()),
            (_, _) => self.eth_client.sender_account(),
        };
        let nonce = self
            .get_next_nonce(&mut transaction, sender_addr, is_gateway)
            .await?;
        let encoded_aggregated_op = self.encode_aggregated_op(
            aggregated_op,
            chain_protocol_version_id,
            use_fusaka_blob_format,
        );

        let eth_tx_predicted_gas = match aggregated_op {
            AggregatedOperation::L2Block(op) => match op {
                L2BlockAggregatedOperation::Precommit { txs, .. } => {
                    L1GasCriterion::total_precommit_gas_amount(is_gateway, txs.len())
                }
            },
            AggregatedOperation::L1Batch(agg_op) => {
                let l1_batch_number_range = agg_op.l1_batch_range();
                let dependency_roots_per_batch = agg_op.dependency_roots_per_batch();
                match agg_op {
                    L1BatchAggregatedOperation::Execute(_) => {
                        L1GasCriterion::total_execute_gas_amount(
                            &mut transaction,
                            l1_batch_number_range.clone(),
                            dependency_roots_per_batch,
                            is_gateway,
                        )
                        .await
                    }
                    L1BatchAggregatedOperation::PublishProofOnchain(prove) => {
                        L1GasCriterion::total_proof_gas_amount(is_gateway)
                            * prove.proofs.len().max(1) as u64
                    }
                    L1BatchAggregatedOperation::Commit(..) => {
                        L1GasCriterion::total_commit_validium_gas_amount(
                            l1_batch_number_range.clone(),
                            is_gateway,
                        )
                    }
                }
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
        match aggregated_op {
            AggregatedOperation::L2Block(agg_op) => {
                transaction
                    .blocks_dal()
                    .set_eth_tx_id_for_l2_blocks(
                        agg_op.l2_blocks_range(),
                        eth_tx.id,
                        agg_op.get_action_type(),
                    )
                    .await
                    .unwrap();
            }
            AggregatedOperation::L1Batch(agg_op) => {
                transaction
                    .blocks_dal()
                    .set_eth_tx_id_for_l1_batches(
                        agg_op.l1_batch_range(),
                        eth_tx.id,
                        aggregated_op.get_action_type(),
                    )
                    .await
                    .unwrap();
            }
        }
        transaction.commit().await.unwrap();
        Ok(eth_tx)
    }

    async fn is_gateway_for_sending_txs(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<bool, EthSenderError> {
        let settlement_layer = if let Some(settlement_layer) = self.settlement_layer {
            Some(settlement_layer)
        } else {
            storage
                .blocks_dal()
                .get_latest_sealed_l1_batch_header()
                .await?
                .map(|header| header.settlement_layer)
        };
        Ok(settlement_layer
            .map(SettlementLayer::is_gateway)
            .unwrap_or(false))
    }

    async fn get_next_nonce(
        &self,
        storage: &mut Connection<'_, Core>,
        from_addr: Address,
        is_gateway: bool,
    ) -> Result<u64, EthSenderError> {
        let db_nonce = storage
            .eth_sender_dal()
            .get_next_nonce(from_addr, is_gateway)
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

    /// Returns `true` if there are batches on the current settlement layer not yet committed.
    /// Used to block gateway migration until all batches are finalized.
    async fn is_waiting_for_batches_with_current_settlement_layer_to_be_committed(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<bool, EthSenderError> {
        let settlement_layer = if let Some(settlement_layer) = self.settlement_layer {
            settlement_layer
        } else if let Some(header) = storage
            .blocks_dal()
            .get_latest_sealed_l1_batch_header()
            .await?
        {
            tracing::info!(
                "Settlement layer for sending txs is unknown during gateway migration; using latest sealed L1 batch settlement layer for wait checks"
            );
            header.settlement_layer
        } else {
            tracing::info!(
                "Settlement layer for sending txs is unknown during gateway migration and there are no sealed L1 batches yet; keep commit/precommit restrictions"
            );
            return Ok(false);
        };

        let has_uncommitted_batches = storage
            .blocks_dal()
            .has_uncommitted_batches_on_settlement_layer(&settlement_layer)
            .await?;

        Ok(has_uncommitted_batches)
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

#[cfg(test)]
mod proof_system_tests {
    use super::*;

    #[tokio::test]
    async fn incompatible_policy_only_pauses_proofs() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };

        use zksync_config::EthConfig;
        use zksync_eth_client::clients::{MockSettlementLayer, L1};
        use zksync_object_store::MockObjectStore;
        use zksync_types::{commitment::L1BatchCommitmentMode, L1BatchNumber, ProtocolVersion};

        use crate::tester::{EthSenderTester, TestL1Batch};

        let pool = ConnectionPool::<Core>::test_pool().await;
        pool.connection()
            .await
            .unwrap()
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion {
                base_system_contracts_hashes: BaseSystemContractsHashes {
                    bootloader: H256::repeat_byte(1),
                    default_aa: H256::repeat_byte(2),
                    evm_emulator: Some(H256::repeat_byte(3)),
                },
                ..Default::default()
            })
            .await
            .unwrap();
        let mut tester = EthSenderTester::new(
            pool.clone(),
            vec![10; 100],
            false,
            false,
            L1BatchCommitmentMode::Rollup,
            SettlementLayer::L1(505.into()),
        )
        .await;
        let config = SenderConfig {
            max_aggregated_blocks_to_commit: 1,
            max_aggregated_blocks_to_execute: 1,
            ..EthConfig::for_tests().sender
        };
        tester.aggregator.aggregator = Aggregator::new(
            config,
            MockObjectStore::arc(),
            false,
            L1BatchCommitmentMode::Rollup,
            pool.clone(),
            SettlementLayer::L1(505.into()),
        )
        .await
        .unwrap();
        TestL1Batch::sealed(&mut tester).await;
        let proven = TestL1Batch::sealed(&mut tester).await;
        let pending = TestL1Batch::sealed(&mut tester).await;
        for number in 0..=pending.number.0 {
            let mut storage = pool.connection().await.unwrap();
            storage
                .blocks_dal()
                .insert_l2_block(&zksync_node_test_utils::create_l2_block(number))
                .await
                .unwrap();
            storage
                .blocks_dal()
                .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(number))
                .await
                .unwrap();
        }
        proven.commit(&mut tester, true).await;
        proven.save_prove_tx(&mut tester).await;

        let fail_read = Arc::new(AtomicBool::new(false));
        let fail_read_in_handler = fail_read.clone();
        let functions = ZkSyncFunctions::default();
        tester.aggregator.eth_client = Box::new(
            MockSettlementLayer::<L1>::builder()
                .with_call_handler(move |call, _| {
                    let data = &call.data.as_ref().unwrap().0;
                    if data[..4] == functions.aggregate3.short_signature() {
                        let result = crate::tests::mock_multicall_response(
                            call,
                            ProtocolVersionId::Version34,
                        );
                        let Token::Array(mut entries) = result else {
                            unreachable!()
                        };
                        if entries.len() == 2 {
                            for entry in &mut entries {
                                *entry = Token::Tuple(vec![
                                    Token::Bool(true),
                                    Token::Bytes(vec![0; 32]),
                                ]);
                            }
                        }
                        return Token::Array(entries);
                    }
                    let disabled = view_function(
                        "disabledProofSystems",
                        vec![],
                        ParamType::Tuple(vec![ParamType::Bool, ParamType::Bool]),
                    );
                    if data[..4] == disabled.short_signature() {
                        if fail_read_in_handler.load(Ordering::Relaxed) {
                            return Token::Tuple(vec![]);
                        }
                        return Token::Tuple(vec![Token::Bool(false), Token::Bool(false)]);
                    }
                    for (name, value, inputs) in [
                        ("supportedProofSystems", 3, vec![]),
                        ("requiredProofSystems", 3, vec![ParamType::Uint(8)]),
                        ("acceptedProofType", 4, vec![]),
                    ] {
                        if data[..4]
                            == view_function(name, inputs, ParamType::Uint(256)).short_signature()
                        {
                            return Token::Uint(value.into());
                        }
                    }
                    if functions
                        .verifier_contract
                        .functions_by_name("verificationKeyHash")
                        .unwrap()
                        .iter()
                        .any(|function| data[..4] == function.short_signature())
                    {
                        return Token::FixedBytes(vec![0; 32]);
                    }
                    panic!("unexpected call: {call:?}");
                })
                .build(),
        );
        tester.aggregator.priority_tree_start_index = Some(0);
        tester
            .aggregator
            .functions
            .verifier_contract
            .functions
            .get_mut("verificationKeyHash")
            .unwrap()
            .reverse();
        assert_eq!(
            tester
                .aggregator
                .get_fflonk_snark_wrapper_vk_hash(Address::zero())
                .await
                .unwrap(),
            Some(H256::zero())
        );
        assert_eq!(
            tester
                .aggregator
                .get_airbender_snark_wrapper_vk_hash(Address::zero())
                .await
                .unwrap(),
            Some(H256::zero())
        );
        let mut storage = pool.connection().await.unwrap();
        assert_eq!(
            tester
                .aggregator
                .sync_proof_systems(&mut storage, Address::zero())
                .await
                .unwrap(),
            Some("incompatible proof system policy")
        );
        tester
            .aggregator
            .loop_iteration(&mut storage)
            .await
            .unwrap();
        assert_eq!(
            storage
                .blocks_dal()
                .get_number_of_last_l1_batch_with_tx(L1BatchAggregatedActionType::Execute)
                .await
                .unwrap(),
            Some(proven.number)
        );
        tester
            .aggregator
            .loop_iteration(&mut storage)
            .await
            .unwrap();
        assert!(storage
            .blocks_dal()
            .get_eth_commit_tx_id(pending.number)
            .await
            .unwrap()
            .is_some());
        tester
            .aggregator
            .loop_iteration(&mut storage)
            .await
            .unwrap();
        assert_eq!(
            storage
                .blocks_dal()
                .get_last_l1_batch_with_prove_tx()
                .await
                .unwrap(),
            proven.number
        );

        tester.aggregator.config.prover = ProverType::Both;
        assert_eq!(
            tester
                .aggregator
                .sync_proof_systems(&mut storage, Address::zero())
                .await
                .unwrap(),
            None
        );
        tester
            .aggregator
            .loop_iteration(&mut storage)
            .await
            .unwrap();
        assert_eq!(
            storage
                .blocks_dal()
                .get_last_l1_batch_with_prove_tx()
                .await
                .unwrap(),
            L1BatchNumber(2)
        );
        fail_read.store(true, Ordering::Relaxed);
        assert!(tester
            .aggregator
            .loop_iteration(&mut storage)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn proof_notifications_pause_until_l1_matches_and_survive_restart() {
        use zksync_types::server_notification::ProofSystemStatusNotification;
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        for current in 0..=2 {
            assert!(proof_systems_match_notification(&mut storage, current)
                .await
                .unwrap());
        }
        for (block, expected) in [1, 0, 2, 0].into_iter().enumerate() {
            storage
                .server_notifications_dal()
                .save_proof_system_status_notification(
                    zksync_types::L1BlockNumber(block as u32 + 1),
                    ProofSystemStatusNotification {
                        disabled_proof_systems: expected,
                        log_index: 0,
                    },
                )
                .await
                .unwrap();
            drop(storage);
            storage = pool.connection().await.unwrap();
            for current in 0..=2 {
                assert_eq!(
                    proof_systems_match_notification(&mut storage, current)
                        .await
                        .unwrap(),
                    current == expected
                );
            }
        }
    }

    #[test]
    fn decodes_disabled_proof_systems_struct() {
        use zksync_l1_contract_interface::Detokenize;
        let function = view_function(
            "disabledProofSystems",
            vec![],
            ParamType::Tuple(vec![ParamType::Bool, ParamType::Bool]),
        );
        for mask in 0..=3 {
            let output = zksync_types::ethabi::encode(&[Token::Tuple(vec![
                Token::Bool(mask & 1 != 0),
                Token::Bool(mask & 2 != 0),
            ])]);
            let decoded = function.decode_output(&output).unwrap();
            assert_eq!(DisabledProofSystems::from_tokens(decoded).unwrap().0, mask);
        }
        assert!(DisabledProofSystems::from_tokens(vec![Token::Uint(0.into())]).is_err());
    }

    #[test]
    fn follows_the_onchain_mask_without_skipping_required_proofs() {
        for (mask, prover) in [
            (0, ProverType::Both),
            (1, ProverType::Airbender),
            (2, ProverType::Boojum),
        ] {
            assert_eq!(
                resolve_prover_policy(
                    ProverType::Both,
                    mask.into(),
                    3.into(),
                    (3 ^ mask).into(),
                    4.into()
                )
                .unwrap(),
                prover
            );
            assert_eq!(
                resolve_prover_policy(prover, mask.into(), 3.into(), (3 ^ mask).into(), 4.into())
                    .unwrap(),
                prover
            );
        }
        for mask in [0, 2] {
            assert_eq!(
                resolve_prover_policy(ProverType::Both, mask.into(), 1.into(), 1.into(), 4.into())
                    .unwrap(),
                ProverType::Boojum
            );
        }
        assert_eq!(
            resolve_prover_policy(ProverType::Both, 0.into(), 2.into(), 2.into(), 4.into())
                .unwrap(),
            ProverType::Airbender
        );
        for (configured, mask, supported, required, proof_type) in [
            (ProverType::Boojum, 0, 3, 3, 4),
            (ProverType::Airbender, 0, 3, 3, 4),
            (ProverType::Both, 0, 1, 3, 4),
            (ProverType::Both, 1, 1, 0, 4),
            (ProverType::Both, 0, 0, 0, 4),
            (ProverType::Both, 0, 4, 4, 4),
            (ProverType::Both, 3, 3, 0, 4),
            (ProverType::Both, 0, 3, 1, 4),
            (ProverType::Both, 0, 3, 3, 2),
        ] {
            assert!(resolve_prover_policy(
                configured,
                mask.into(),
                supported.into(),
                required.into(),
                proof_type.into()
            )
            .is_err());
        }
    }
}
