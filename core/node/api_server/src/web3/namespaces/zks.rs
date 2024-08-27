use std::{collections::HashMap, convert::TryInto};

use anyhow::Context as _;
use once_cell::sync::Lazy;
use zksync_crypto_primitives::hasher::{keccak::KeccakHasher, Hasher};
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_metadata_calculator::api_server::TreeApiError;
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_multivm::interface::VmExecutionResultAndLogs;
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    api::{
        state_override::StateOverride, BlockDetails, BridgeAddresses, GetLogsFilter,
        L1BatchDetails, L2ToL1LogProof, LeafAggProof, Proof, ProtocolVersion, StorageProof,
        TransactionDetails,
    },
    ethabi,
    fee::Fee,
    fee_model::{FeeParams, PubdataIndependentBatchFeeModelInput},
    l1::L1Tx,
    l2::L2Tx,
    l2_to_l1_log::{l2_to_l1_logs_tree_size, L2ToL1Log},
    tokens::ETHEREUM_ADDRESS,
    transaction_request::CallRequest,
    utils::storage_key_for_standard_token_balance,
    web3::Bytes,
    AccountTreeId, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId, StorageKey,
    Transaction, L1_MESSENGER_ADDRESS, L2_BASE_TOKEN_ADDRESS, L2_MESSAGE_ROOT_ADDRESS,
    REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256, U64,
};
use zksync_utils::{address_to_h256, h256_to_u256, u256_to_h256};
use zksync_web3_decl::{
    client::{Client, L2},
    error::Web3Error,
    namespaces::ZksNamespaceClient,
    types::{Address, Token, H256},
};

use crate::{
    utils::open_readonly_transaction,
    web3::{backend_jsonrpsee::MethodTracer, metrics::API_METRICS, RpcState},
};

pub static MESSAGE_ROOT_ADDED_CHAIN_EVENT: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "AddedChain",
        &[ethabi::ParamType::Uint(256), ethabi::ParamType::Uint(256)],
    )
});

pub static MESSAGE_ROOT_ADDED_CHAIN_BATCH_ROOT_EVENT: Lazy<H256> = Lazy::new(|| {
    ethabi::long_signature(
        "AppendedChainBatchRoot",
        &[
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::FixedBytes(32),
        ],
    )
});

#[derive(Debug)]
pub(crate) struct ZksNamespace {
    state: RpcState,
}

impl ZksNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    pub async fn estimate_fee_impl(
        &self,
        request: CallRequest,
        state_override: Option<StateOverride>,
    ) -> Result<Fee, Web3Error> {
        let mut request_with_gas_per_pubdata_overridden = request;
        self.state
            .set_nonce_for_call_request(&mut request_with_gas_per_pubdata_overridden)
            .await?;

        if let Some(ref mut eip712_meta) = request_with_gas_per_pubdata_overridden.eip712_meta {
            eip712_meta.gas_per_pubdata = U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE);
        }

        let mut tx = L2Tx::from_request(
            request_with_gas_per_pubdata_overridden.into(),
            self.state.api_config.max_tx_size,
        )?;

        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        tx.common_data.fee.max_priority_fee_per_gas = 0u64.into();
        tx.common_data.fee.gas_per_pubdata_limit = U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE);
        self.estimate_fee(tx.into(), state_override).await
    }

    pub async fn estimate_l1_to_l2_gas_impl(
        &self,
        request: CallRequest,
        state_override: Option<StateOverride>,
    ) -> Result<U256, Web3Error> {
        let mut request_with_gas_per_pubdata_overridden = request;
        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        if let Some(ref mut eip712_meta) = request_with_gas_per_pubdata_overridden.eip712_meta {
            if eip712_meta.gas_per_pubdata == U256::zero() {
                eip712_meta.gas_per_pubdata = REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into();
            }
        }

        let tx: L1Tx = request_with_gas_per_pubdata_overridden
            .try_into()
            .map_err(Web3Error::SerializationError)?;

        let fee = self.estimate_fee(tx.into(), state_override).await?;
        Ok(fee.gas_limit)
    }

    async fn estimate_fee(
        &self,
        tx: Transaction,
        state_override: Option<StateOverride>,
    ) -> Result<Fee, Web3Error> {
        let scale_factor = self.state.api_config.estimate_gas_scale_factor;
        let acceptable_overestimation =
            self.state.api_config.estimate_gas_acceptable_overestimation;

        Ok(self
            .state
            .tx_sender
            .get_txs_fee_in_wei(
                tx,
                scale_factor,
                acceptable_overestimation as u64,
                state_override,
            )
            .await?)
    }

    pub fn get_bridgehub_contract_impl(&self) -> Option<Address> {
        self.state.api_config.user_facing_bridgehub_addr
    }

    pub fn get_main_contract_impl(&self) -> Address {
        self.state.api_config.user_facing_diamond_proxy_addr
    }

    pub fn get_testnet_paymaster_impl(&self) -> Option<Address> {
        self.state.api_config.l2_testnet_paymaster_addr
    }

    pub fn get_native_token_vault_proxy_addr_impl(&self) -> Option<Address> {
        self.state.api_config.l2_native_token_vault_proxy_addr
    }

    pub fn get_bridge_contracts_impl(&self) -> BridgeAddresses {
        self.state.api_config.bridge_addresses.clone()
    }

    pub fn l1_chain_id_impl(&self) -> U64 {
        U64::from(*self.state.api_config.l1_chain_id)
    }

    pub async fn get_confirmed_tokens_impl(
        &self,
        from: u32,
        limit: u8,
    ) -> Result<Vec<Token>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let tokens = storage
            .tokens_web3_dal()
            .get_well_known_tokens()
            .await
            .map_err(DalError::generalize)?;

        let tokens = tokens
            .into_iter()
            .skip(from as usize)
            .take(limit.into())
            .map(|token_info| Token {
                l1_address: token_info.l1_address,
                l2_address: token_info.l2_address,
                name: token_info.metadata.name,
                symbol: token_info.metadata.symbol,
                decimals: token_info.metadata.decimals,
            })
            .collect();
        Ok(tokens)
    }

    pub async fn get_all_account_balances_impl(
        &self,
        address: Address,
    ) -> Result<HashMap<Address, U256>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let tokens = storage
            .tokens_dal()
            .get_all_l2_token_addresses()
            .await
            .map_err(DalError::generalize)?;
        let hashed_balance_keys = tokens.iter().map(|&token_address| {
            let token_account = AccountTreeId::new(if token_address == ETHEREUM_ADDRESS {
                L2_BASE_TOKEN_ADDRESS
            } else {
                token_address
            });
            let hashed_key =
                storage_key_for_standard_token_balance(token_account, &address).hashed_key();
            (hashed_key, (hashed_key, token_address))
        });
        let (hashed_balance_keys, hashed_key_to_token_address): (Vec<_>, HashMap<_, _>) =
            hashed_balance_keys.unzip();

        let balance_values = storage
            .storage_web3_dal()
            .get_values(&hashed_balance_keys)
            .await
            .map_err(DalError::generalize)?;

        let balances = balance_values
            .into_iter()
            .filter_map(|(hashed_key, balance)| {
                let balance = h256_to_u256(balance);
                if balance.is_zero() {
                    return None;
                }
                Some((hashed_key_to_token_address[&hashed_key], balance))
            })
            .collect();
        Ok(balances)
    }

    pub async fn get_l2_to_l1_msg_proof_impl(
        &self,
        block_number: L2BlockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        let Some(l1_batch_number) = storage
            .blocks_web3_dal()
            .get_l1_batch_number_of_l2_block(block_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };
        let (first_l2_block_of_l1_batch, _) = storage
            .blocks_web3_dal()
            .get_l2_block_range_of_l1_batch(l1_batch_number)
            .await
            .map_err(DalError::generalize)?
            .context("L1 batch should contain at least one L2 block")?;

        // Position of l1 log in L1 batch relative to logs with identical data
        let l1_log_relative_position = if let Some(l2_log_position) = l2_log_position {
            let logs = storage
                .events_web3_dal()
                .get_logs(
                    GetLogsFilter {
                        from_block: first_l2_block_of_l1_batch,
                        to_block: block_number,
                        addresses: vec![L1_MESSENGER_ADDRESS],
                        topics: vec![(2, vec![address_to_h256(&sender)]), (3, vec![msg])],
                    },
                    self.state.api_config.req_entities_limit,
                )
                .await
                .map_err(DalError::generalize)?;
            let maybe_pos = logs.iter().position(|event| {
                event.block_number == Some(block_number.0.into())
                    && event.log_index == Some(l2_log_position.into())
            });
            match maybe_pos {
                Some(pos) => pos,
                None => return Ok(None),
            }
        } else {
            0
        };

        let log_proof = self
            .get_l2_to_l1_log_proof_inner(
                &mut storage,
                l1_batch_number,
                l1_log_relative_position,
                |log| {
                    log.sender == L1_MESSENGER_ADDRESS
                        && log.key == address_to_h256(&sender)
                        && log.value == msg
                },
            )
            .await?;
        Ok(log_proof)
    }

    async fn get_l2_to_l1_log_proof_inner(
        &self,
        storage: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
        index_in_filtered_logs: usize,
        log_filter: impl Fn(&L2ToL1Log) -> bool,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let all_l1_logs_in_batch = storage
            .blocks_web3_dal()
            .get_l2_to_l1_logs(l1_batch_number)
            .await
            .map_err(DalError::generalize)?;

        let Some((l1_log_index, _)) = all_l1_logs_in_batch
            .iter()
            .enumerate()
            .filter(|(_, log)| log_filter(log))
            .nth(index_in_filtered_logs)
        else {
            return Ok(None);
        };

        let Some(batch) = storage
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        let Some(batch_meta) = storage
            .blocks_dal()
            .get_l1_batch_metadata(l1_batch_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        let merkle_tree_leaves = all_l1_logs_in_batch.iter().map(L2ToL1Log::to_bytes);

        let protocol_version = batch
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);
        let tree_size = l2_to_l1_logs_tree_size(protocol_version);

        let (root, mut proof) = MiniMerkleTree::new(merkle_tree_leaves, Some(tree_size))
            .merkle_root_and_path(l1_log_index);

        // For now it is always 0
        let aggregated_root = batch_meta.metadata.aggregation_root;
        let final_root = KeccakHasher.compress(&root, &aggregated_root);
        proof.push(aggregated_root);

        println!("Trying to get the final proof! {}", l1_batch_number);

        // FIXME Definitely refactor all of it
        const EXPECTED_SYNC_LAYER_CHAIN_ID: u64 = 270;

        let mut log_leaf_proof = LogLeafProof::new(proof);

        let settlement_layer: u64 = std::env::var("ETH_CLIENT_CHAIN_ID")
            .unwrap()
            .parse()
            .unwrap();

        if settlement_layer == EXPECTED_SYNC_LAYER_CHAIN_ID {
            println!("\nI am on sync layer!!\n");
            // We are on top of sync layer.
            // Maaybe there is an aggregation proof waiting

            // Create a client for pinging the RPC.
            let client: Client<L2> = Client::http(
                std::env::var("GATEWAY_API_WEB3_JSON_RPC_HTTP_URL")
                    .unwrap()
                    .parse()
                    .unwrap(),
            )?
            .for_network(L2::from(L2ChainId(self.state.api_config.l1_chain_id.0)))
            .build();

            println!("\ncreated client!!\n");

            let proof = client
                .get_aggregated_batch_inclusion_proof(
                    L2_MESSAGE_ROOT_ADDRESS,
                    l1_batch_number,
                    self.state.api_config.l2_chain_id.0 as u32,
                )
                .await
                .unwrap_or_else(|err| {
                    panic!("Failed reaching to the SL: {:#?}", err);
                });
            println!("Proof: {:#?}", proof);

            if let Some(proof) = proof {
                println!("Found proof for my own batch :{:#?}", proof);

                log_leaf_proof.append_aggregation_layer(proof);
            } else {
                return Ok(None);
            }
        }

        let proof = log_leaf_proof.encode();

        Ok(Some(L2ToL1LogProof {
            proof,
            root: final_root,
            id: l1_log_index as u32,
        }))
    }

    pub async fn get_l2_to_l1_log_proof_impl(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let Some((l1_batch_number, l1_batch_tx_index)) = storage
            .blocks_web3_dal()
            .get_l1_batch_info_for_tx(tx_hash)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        let log_proof = self
            .get_l2_to_l1_log_proof_inner(
                &mut storage,
                l1_batch_number,
                index.unwrap_or(0),
                |log| log.tx_number_in_block == l1_batch_tx_index,
            )
            .await?;
        Ok(log_proof)
    }

    async fn l1_batch_number_with_agg_batch(
        &self,
        storage: &mut Connection<'_, Core>,
        latest_sealed_block_number: L2BlockNumber,
        message_root_addr: Address,
        batch_number: L1BatchNumber,
        chain_id: u32,
    ) -> Result<Option<u32>, Web3Error> {
        let add_chain_logs = storage
            .events_web3_dal()
            .get_logs(
                GetLogsFilter {
                    // FIXME: this is somewhat inefficient, better ways need to be created
                    from_block: 0.into(),
                    to_block: latest_sealed_block_number,
                    addresses: vec![message_root_addr],
                    topics: vec![
                        (1, vec![*MESSAGE_ROOT_ADDED_CHAIN_BATCH_ROOT_EVENT]),
                        (2, vec![u256_to_h256(U256::from(chain_id))]),
                        (3, vec![u256_to_h256(U256::from(batch_number.0))]),
                    ],
                },
                self.state.api_config.req_entities_limit,
            )
            .await
            .map_err(DalError::generalize)?;

        println!("LOGS = {:#?}", add_chain_logs);

        // At most one such log is expected
        assert!(add_chain_logs.len() <= 1);

        if add_chain_logs.is_empty() {
            return Ok(None);
        }

        let Some(l1_batch_number) = add_chain_logs[0].l1_batch_number else {
            return Ok(None);
        };

        Ok(Some(l1_batch_number.as_u32()))
    }

    // FIXME: `message_root_addr` is most often constant. The only reason we may want to provide a custom value is
    // for L1, but at this point maybe it could known from the config, not sure.
    pub async fn get_aggregated_batch_inclusion_proof_impl(
        &self,
        message_root_addr: Address,
        searched_batch_number: L1BatchNumber,
        searched_chain_id: u32,
    ) -> Result<Option<LeafAggProof>, Web3Error> {
        println!("heee");
        let mut storage = self.state.acquire_connection().await?;

        // Proofs only available for finalized batches
        let latest_sealed_block_number = storage
            .blocks_dal()
            .get_last_sealed_l2_block_header()
            .await
            .map_err(DalError::generalize)?
            .map(|header| header.number)
            .unwrap_or_default();

        let l1_batch_number_with_agg_batch = self
            .l1_batch_number_with_agg_batch(
                &mut storage,
                latest_sealed_block_number,
                message_root_addr,
                searched_batch_number,
                searched_chain_id,
            )
            .await?;
        println!("hee2");
        let Some(l1_batch_number_with_agg_batch) = l1_batch_number_with_agg_batch else {
            return Ok(None);
        };
        println!("hee3");

        let local_msg_root = storage
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(l1_batch_number_with_agg_batch))
            .await
            .map_err(DalError::generalize)?
            .map(|metadata| metadata.metadata.local_root);

        let Some(local_msg_root) = local_msg_root else {
            return Ok(None);
        };

        // FIXME: move as api config
        // Firstly, let's grab all events that correspond to batch being inserted into the chain_id tree.
        let add_chain_logs = storage
            .events_web3_dal()
            .get_logs(
                GetLogsFilter {
                    // FIXME: this is somewhat inefficient, better ways need to be created
                    from_block: 0.into(),
                    to_block: latest_sealed_block_number,
                    addresses: vec![message_root_addr],
                    topics: vec![(1, vec![*MESSAGE_ROOT_ADDED_CHAIN_EVENT])],
                },
                self.state.api_config.req_entities_limit,
            )
            .await
            .map_err(DalError::generalize)?;

        println!("hee4");
        let add_batch_logs = storage
            .events_web3_dal()
            .get_logs(
                GetLogsFilter {
                    // FIXME: this is somewhat inefficient, better ways need to be created
                    from_block: 0.into(),
                    to_block: latest_sealed_block_number,
                    addresses: vec![message_root_addr],
                    topics: vec![(1, vec![*MESSAGE_ROOT_ADDED_CHAIN_BATCH_ROOT_EVENT])],
                },
                self.state.api_config.req_entities_limit,
            )
            .await
            .map_err(DalError::generalize)?;

        println!("Add batch logs: {:#?}", add_batch_logs);

        let mut full_chain_merkle_tree =
            MiniMerkleTree::<[u8; 96], KeccakHasher>::new(Vec::<[u8; 96]>::new().into_iter(), None);

        let mut batch_leaf_proof = vec![];
        let mut batch_leaf_proof_mask = None;

        let mut chain_id_leaf_proof_mask = None;

        for (i, chain_add_log) in add_chain_logs.into_iter().enumerate() {
            let Some(batch_num) = chain_add_log.l1_batch_number else {
                continue;
            };
            let batch_num: u32 = batch_num.as_u32();

            if batch_num > l1_batch_number_with_agg_batch {
                continue;
            };

            let chain_id = h256_to_u256(chain_add_log.topics[1]);
            let index = h256_to_u256(chain_add_log.topics[2]);

            if chain_id.as_u32() == searched_chain_id {
                chain_id_leaf_proof_mask = Some(i);
            }

            // Double check index correctness
            assert_eq!(U256::from(i), index);

            let mut chain_id_merkle_tree = MiniMerkleTree::<[u8; 96], KeccakHasher>::new(
                Vec::<[u8; 96]>::new().into_iter(),
                None,
            );

            let mut cnt = 0;

            for add_batch_log in add_batch_logs.iter() {
                let Some(batch_num) = add_batch_log.l1_batch_number else {
                    continue;
                };
                let batch_num: u32 = batch_num.as_u32();

                if batch_num > l1_batch_number_with_agg_batch {
                    continue;
                };

                let chain_id = h256_to_u256(add_batch_log.topics[1]);
                let batch_number = h256_to_u256(add_batch_log.topics[2]);

                if chain_id.as_u32() != searched_chain_id {
                    continue;
                }

                if batch_number.as_u32() == searched_batch_number.0
                    && chain_id.as_u32() == searched_chain_id
                {
                    println!("relevat batch found! {:#?}", add_batch_log);
                    batch_leaf_proof_mask = Some(cnt);
                }

                println!("appended log: {:#?}", add_batch_log);

                let batch_root = H256::from_slice(&add_batch_log.data.0);
                chain_id_merkle_tree
                    .push(Self::batch_leaf_preimage(batch_root, batch_number.as_u32()));

                println!("new batch root = {:#?}", chain_id_merkle_tree.merkle_root());

                cnt += 1;
            }

            if chain_id.as_u32() == searched_chain_id {
                let Some(batch_leaf_proof_mask) = batch_leaf_proof_mask else {
                    return Ok(None);
                };

                let result = chain_id_merkle_tree.merkle_root_and_path(batch_leaf_proof_mask);
                let root = result.0;
                batch_leaf_proof = result.1;

                println!(
                    "EXPECTED ROOT FOR {} / {} = {}",
                    searched_chain_id, batch_leaf_proof_mask, root
                );
            }

            full_chain_merkle_tree.push(Self::chain_id_leaf_preimage(
                chain_id_merkle_tree.merkle_root(),
                chain_id.as_u32(),
            ));
        }

        let Some(chain_id_leaf_proof_mask) = chain_id_leaf_proof_mask else {
            return Ok(None);
        };

        let mut chain_id_leaf_proof = full_chain_merkle_tree
            .merkle_root_and_path(chain_id_leaf_proof_mask)
            .1;

        chain_id_leaf_proof.push(local_msg_root);
        let chain_id_leaf_proof_mask =
            chain_id_leaf_proof_mask | (1 << (chain_id_leaf_proof.len() - 1));

        let full_agg_root = full_chain_merkle_tree.merkle_root();

        let result = LeafAggProof {
            batch_leaf_proof,
            batch_leaf_proof_mask: batch_leaf_proof_mask.unwrap().into(),
            chain_id_leaf_proof,
            chain_id_leaf_proof_mask: chain_id_leaf_proof_mask.into(),
            local_msg_root,
            sl_batch_number: l1_batch_number_with_agg_batch.into(),
        };

        println!(
            "\n\n FULL AGG ROOT FOR BATCH = {:#?}\n\n",
            hex::encode(full_agg_root.0)
        );

        Ok(Some(result))
    }

    pub fn batch_leaf_preimage(batch_root: H256, batch_number: u32) -> [u8; 96] {
        let prefix =
            hex::decode("d82fec4a37cbdc47f1e5cc4ad64deacf34a48e6f7c61fa5b68fd58e543259cf4")
                .unwrap();
        let mut full_preimage = [0u8; 96];

        full_preimage[0..32].copy_from_slice(&prefix);
        full_preimage[32..64].copy_from_slice(&batch_root.0);
        full_preimage[64..96].copy_from_slice(&u256_to_h256(batch_number.into()).0);

        full_preimage
    }

    pub fn chain_id_leaf_preimage(chain_root: H256, chain_id: u32) -> [u8; 96] {
        let prefix =
            hex::decode("39bc69363bb9e26cf14240de4e22569e95cf175cfbcf1ade1a47a253b4bf7f61")
                .unwrap();
        let mut full_preimage = [0u8; 96];

        full_preimage[0..32].copy_from_slice(&prefix);
        full_preimage[32..64].copy_from_slice(&chain_root.0);
        full_preimage[64..96].copy_from_slice(&u256_to_h256(chain_id.into()).0);

        full_preimage
    }

    pub async fn get_l1_batch_number_impl(&self) -> Result<U64, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .map_err(DalError::generalize)?
            .ok_or(Web3Error::NoBlock)?;
        Ok(l1_batch_number.0.into())
    }

    pub async fn get_l2_block_range_impl(
        &self,
        batch: L1BatchNumber,
    ) -> Result<Option<(U64, U64)>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(batch, &mut storage)
            .await?;
        let range = storage
            .blocks_web3_dal()
            .get_l2_block_range_of_l1_batch(batch)
            .await
            .map_err(DalError::generalize)?;
        Ok(range.map(|(min, max)| (U64::from(min.0), U64::from(max.0))))
    }

    pub async fn get_block_details_impl(
        &self,
        block_number: L2BlockNumber,
    ) -> Result<Option<BlockDetails>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        Ok(storage
            .blocks_web3_dal()
            .get_block_details(block_number)
            .await
            .map_err(DalError::generalize)?)
    }

    pub async fn get_raw_block_transactions_impl(
        &self,
        block_number: L2BlockNumber,
    ) -> Result<Vec<Transaction>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        Ok(storage
            .transactions_web3_dal()
            .get_raw_l2_block_transactions(block_number)
            .await
            .map_err(DalError::generalize)?)
    }

    pub async fn get_transaction_details_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionDetails>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        // Open a readonly transaction to have a consistent view of Postgres
        let mut storage = open_readonly_transaction(&mut storage).await?;
        let mut tx_details = storage
            .transactions_web3_dal()
            .get_transaction_details(hash)
            .await
            .map_err(DalError::generalize)?;

        if tx_details.is_none() {
            tx_details = self
                .state
                .tx_sink()
                .lookup_tx_details(&mut storage, hash)
                .await?;
        }
        Ok(tx_details)
    }

    pub async fn get_l1_batch_details_impl(
        &self,
        batch_number: L1BatchNumber,
    ) -> Result<Option<L1BatchDetails>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(batch_number, &mut storage)
            .await?;

        Ok(storage
            .blocks_web3_dal()
            .get_l1_batch_details(batch_number)
            .await
            .map_err(DalError::generalize)?)
    }

    pub async fn get_bytecode_by_hash_impl(
        &self,
        hash: H256,
    ) -> Result<Option<Vec<u8>>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        Ok(storage
            .factory_deps_dal()
            .get_sealed_factory_dep(hash)
            .await
            .map_err(DalError::generalize)?)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_fee_params_impl(&self) -> FeeParams {
        self.state
            .tx_sender
            .0
            .batch_fee_input_provider
            .get_fee_model_params()
    }

    pub async fn get_protocol_version_impl(
        &self,
        version_id: Option<u16>,
    ) -> Result<Option<ProtocolVersion>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let protocol_version = if let Some(id) = version_id {
            storage
                .protocol_versions_web3_dal()
                .get_protocol_version_by_id(id)
                .await
                .map_err(DalError::generalize)?
        } else {
            Some(
                storage
                    .protocol_versions_web3_dal()
                    .get_latest_protocol_version()
                    .await
                    .map_err(DalError::generalize)?,
            )
        };
        Ok(protocol_version)
    }

    pub async fn get_proofs_impl(
        &self,
        address: Address,
        keys: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<Proof>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(l1_batch_number, &mut storage)
            .await?;
        let hashed_keys = keys
            .iter()
            .map(|key| StorageKey::new(AccountTreeId::new(address), *key).hashed_key_u256())
            .collect();
        let tree_api = self
            .state
            .tree_api
            .as_deref()
            .ok_or(Web3Error::MethodNotImplemented)?;
        let proofs_result = tree_api.get_proofs(l1_batch_number, hashed_keys).await;
        let proofs = match proofs_result {
            Ok(proofs) => proofs,
            Err(TreeApiError::NotReady(_)) => return Err(Web3Error::TreeApiUnavailable),
            Err(TreeApiError::NoVersion(err)) => {
                return if err.missing_version > err.version_count {
                    Ok(None)
                } else {
                    Err(Web3Error::InternalError(anyhow::anyhow!(
                        "L1 batch #{l1_batch_number} is pruned in Merkle tree, but not in Postgres"
                    )))
                };
            }
            Err(TreeApiError::Internal(err)) => return Err(Web3Error::InternalError(err)),
            Err(_) => {
                // This branch is not expected to be executed, but has to be provided since the error is non-exhaustive.
                return Err(Web3Error::InternalError(anyhow::anyhow!(
                    "Unspecified tree API error"
                )));
            }
        };

        let storage_proof = proofs
            .into_iter()
            .zip(keys)
            .map(|(proof, key)| StorageProof {
                key,
                proof: proof.merkle_path,
                value: proof.value,
                index: proof.index,
            })
            .collect();

        Ok(Some(Proof {
            address,
            storage_proof,
        }))
    }

    pub fn get_base_token_l1_address_impl(&self) -> Result<Address, Web3Error> {
        self.state
            .api_config
            .base_token_address
            .ok_or(Web3Error::MethodNotImplemented)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_batch_fee_input_impl(
        &self,
    ) -> Result<PubdataIndependentBatchFeeModelInput, Web3Error> {
        Ok(self
            .state
            .tx_sender
            .0
            .batch_fee_input_provider
            .get_batch_fee_input()
            .await?
            .into_pubdata_independent())
    }

    #[tracing::instrument(skip(self, tx_bytes))]
    pub async fn send_raw_transaction_with_detailed_output_impl(
        &self,
        tx_bytes: Bytes,
    ) -> Result<(H256, VmExecutionResultAndLogs), Web3Error> {
        let (mut tx, hash) = self.state.parse_transaction_bytes(&tx_bytes.0)?;
        tx.set_input(tx_bytes.0, hash);

        let submit_result = self.state.tx_sender.submit_tx(tx).await;
        submit_result.map(|result| (hash, result.1)).map_err(|err| {
            tracing::debug!("Send raw transaction error: {err}");
            API_METRICS.submit_tx_error[&err.prom_error_code()].inc();
            err.into()
        })
    }
}

struct TreeLeafProof {
    leaf_proof: Vec<H256>,
    batch_leaf_proof: Option<LeafAggProof>,
}

impl TreeLeafProof {
    pub fn encode(self) -> Vec<H256> {
        const SUPPORTED_METADATA_VERSION: u8 = 1;

        let log_leaf_proof_len = self.leaf_proof.len();

        let (batch_leaf_proof_len, batch_leaf_proof) = if let Some(x) = self.batch_leaf_proof {
            x.encode()
        } else {
            (0, vec![])
        };

        assert!(log_leaf_proof_len < u8::MAX as usize);
        assert!(batch_leaf_proof_len < u8::MAX as u32);

        let mut metadata = [0u8; 32];
        metadata[0] = SUPPORTED_METADATA_VERSION;
        metadata[1] = log_leaf_proof_len as u8;
        metadata[2] = batch_leaf_proof_len as u8;

        let mut result = vec![H256(metadata)];

        result.extend(self.leaf_proof);
        result.extend(batch_leaf_proof);

        result
    }
}

struct LogLeafProof {
    agg_proofs: Vec<TreeLeafProof>,
}

impl LogLeafProof {
    pub fn new(leaf_proof: Vec<H256>) -> Self {
        let bottom_layer = TreeLeafProof {
            leaf_proof,
            batch_leaf_proof: None,
        };

        Self {
            agg_proofs: vec![bottom_layer],
        }
    }

    pub fn encode(self) -> Vec<H256> {
        let mut result = vec![];
        for i in self.agg_proofs {
            result.extend(i.encode());
        }
        result
    }

    pub fn append_aggregation_layer(&mut self, proof: LeafAggProof) {
        let chain_id_leaf_proof = proof.chain_id_leaf_proof.clone();
        self.agg_proofs.last_mut().unwrap().batch_leaf_proof = Some(proof);
        self.agg_proofs.push(TreeLeafProof {
            leaf_proof: chain_id_leaf_proof,
            batch_leaf_proof: None,
        })
    }
}
