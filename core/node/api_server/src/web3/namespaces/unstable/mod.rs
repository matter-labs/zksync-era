use chrono::{DateTime, Utc};
use itertools::Itertools;
use utils::{
    chain_id_leaf_preimage, get_chain_count, get_chain_id_from_index, get_chain_root_from_id,
};
use zksync_crypto_primitives::hasher::keccak::KeccakHasher;
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_multivm::{interface::VmEvent, zk_evm_latest::ethereum_types::U64};
use zksync_types::{
    api,
    api::{
        ChainAggProof, DataAvailabilityDetails, GatewayMigrationStatus, L1ToL2TxsStatus, TeeProof,
        TransactionDetailedResult, TransactionExecutionInfo,
    },
    server_notification::GatewayMigrationState,
    tee_types::TeeType,
    web3,
    web3::Bytes,
    L1BatchNumber, L2BlockNumber, L2ChainId,
};
use zksync_web3_decl::{error::Web3Error, types::H256};

use crate::{
    execution_sandbox::BlockArgs,
    web3::{backend_jsonrpsee::MethodTracer, RpcState},
};

mod utils;

#[derive(Debug)]
pub(crate) struct UnstableNamespace {
    state: RpcState,
}

impl UnstableNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    pub async fn transaction_execution_info_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionExecutionInfo>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        Ok(storage
            .transactions_web3_dal()
            .get_unstable_transaction_execution_info(hash)
            .await
            .map_err(DalError::generalize)?
            .map(|execution_info| TransactionExecutionInfo { execution_info }))
    }

    pub async fn get_tee_proofs_impl(
        &self,
        l1_batch_number: L1BatchNumber,
        tee_type: Option<TeeType>,
    ) -> Result<Vec<TeeProof>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let proofs = storage
            .tee_proof_generation_dal()
            .get_tee_proofs(l1_batch_number, tee_type)
            .await
            .map_err(DalError::generalize)?
            .into_iter()
            .map(|proof| TeeProof {
                l1_batch_number,
                tee_type,
                pubkey: proof.pubkey,
                signature: proof.signature,
                proof: proof.proof,
                proved_at: DateTime::<Utc>::from_naive_utc_and_offset(proof.updated_at, Utc),
                status: proof.status,
                attestation: proof.attestation,
            })
            .collect::<Vec<_>>();

        Ok(proofs)
    }

    pub async fn get_chain_log_proof_impl(
        &self,
        l1_batch_number: L1BatchNumber,
        l2_chain_id: L2ChainId,
    ) -> Result<Option<ChainAggProof>, Web3Error> {
        let mut connection = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(l1_batch_number, &mut connection)
            .await?;

        let l2_block_number = match connection
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(l1_batch_number)
            .await
            .map_err(DalError::generalize)?
            .map(|(_, end_block)| end_block)
        {
            Some(block_num) => block_num,
            None => return Ok(None),
        };

        let mut chain_log_proof = match self
            .get_chain_log_proof_inner(&mut connection, l2_block_number, l2_chain_id)
            .await?
        {
            Some(chain_log_proof) => chain_log_proof,
            None => return Ok(None),
        };

        let Some(local_root) = connection
            .blocks_dal()
            .get_l1_batch_local_root(l1_batch_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        // Chain tree is the right subtree of the aggregated tree.
        // We append root of the left subtree to form full proof.
        chain_log_proof.chain_id_leaf_proof_mask |= 1 << chain_log_proof.chain_id_leaf_proof.len();
        chain_log_proof.chain_id_leaf_proof.push(local_root);

        Ok(Some(chain_log_proof))
    }

    pub async fn get_chain_log_proof_until_msg_root_impl(
        &self,
        l2_block_number: L2BlockNumber,
        l2_chain_id: L2ChainId,
    ) -> Result<Option<ChainAggProof>, Web3Error> {
        let mut connection = self.state.acquire_connection().await?;

        self.get_chain_log_proof_inner(&mut connection, l2_block_number, l2_chain_id)
            .await
    }

    // This method is used for both get_chain_log_proof and get_chain_log_proof_until_msg_root.
    async fn get_chain_log_proof_inner(
        &self,
        connection: &mut Connection<'_, Core>,
        l2_block_number: L2BlockNumber,
        l2_chain_id: L2ChainId,
    ) -> Result<Option<ChainAggProof>, Web3Error> {
        self.state
            .start_info
            .ensure_not_pruned(l2_block_number, connection)
            .await?;

        let chain_count_integer = get_chain_count(connection, l2_block_number).await?;

        let mut chain_ids = Vec::new();
        for chain_index in 0..chain_count_integer {
            chain_ids
                .push(get_chain_id_from_index(connection, chain_index, l2_block_number).await?);
        }

        let Some((chain_id_leaf_proof_mask, _)) = chain_ids
            .iter()
            .find_position(|id| **id == H256::from_low_u64_be(l2_chain_id.as_u64()))
        else {
            return Ok(None);
        };

        let mut leaves = Vec::new();
        for chain_id in chain_ids {
            let chain_root = get_chain_root_from_id(connection, chain_id, l2_block_number).await?;
            leaves.push(chain_id_leaf_preimage(chain_root, chain_id));
        }

        let chain_merkle_tree =
            MiniMerkleTree::<[u8; 96], KeccakHasher>::new(leaves.into_iter(), None);

        let chain_id_leaf_proof = chain_merkle_tree
            .merkle_root_and_path(chain_id_leaf_proof_mask)
            .1;

        Ok(Some(ChainAggProof {
            chain_id_leaf_proof,
            chain_id_leaf_proof_mask: chain_id_leaf_proof_mask as u64,
        }))
    }

    pub async fn get_unconfirmed_txs_count_impl(&self) -> Result<usize, Web3Error> {
        let mut connection = self.state.acquire_connection().await?;

        let result = connection
            .eth_sender_dal()
            .get_unconfirmed_txs_count()
            .await
            .map_err(DalError::generalize)?;

        Ok(result)
    }

    pub async fn get_data_availability_details_impl(
        &self,
        batch: L1BatchNumber,
    ) -> Result<Option<DataAvailabilityDetails>, Web3Error> {
        let mut connection = self.state.acquire_connection().await?;
        let Some(da_details) = connection
            .data_availability_dal()
            .get_da_details_by_batch_number(batch)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        Ok(Some(DataAvailabilityDetails {
            pubdata_type: da_details.pubdata_type,
            blob_id: da_details.blob_id,
            inclusion_data: da_details.inclusion_data,
            sent_at: da_details.sent_at,
            l2_da_validator: da_details.l2_da_validator,
        }))
    }

    pub fn supports_unsafe_deposit_filter_impl(&self) -> bool {
        true
    }

    pub async fn l1_to_l2_txs_status_impl(&self) -> Result<L1ToL2TxsStatus, Web3Error> {
        let mut connection = self.state.acquire_connection().await?;
        let l1_to_l2_txs_in_mempool = connection
            .transactions_dal()
            .get_priority_txs_in_mempool()
            .await
            .map_err(DalError::generalize)?;

        Ok(L1ToL2TxsStatus {
            l1_to_l2_txs_paused: self.state.api_config.l1_to_l2_txs_paused,
            l1_to_l2_txs_in_mempool,
        })
    }

    pub async fn gateway_migration_status_impl(&self) -> Result<GatewayMigrationStatus, Web3Error> {
        let mut connection = self.state.acquire_connection().await?;

        let latest_notification = connection
            .server_notifications_dal()
            .get_latest_gateway_migration_notification()
            .await
            .map_err(DalError::generalize)?;

        let state = GatewayMigrationState::from_sl_and_notification(
            self.state.api_config.settlement_layer,
            latest_notification,
        );

        Ok(GatewayMigrationStatus {
            latest_notification,
            state,
            settlement_layer: self.state.api_config.settlement_layer,
        })
    }

    #[tracing::instrument(skip(self, tx_bytes))]
    pub async fn send_raw_transaction_with_detailed_output_impl(
        &self,
        tx_bytes: Bytes,
    ) -> Result<TransactionDetailedResult, Web3Error> {
        let mut connection = self.state.acquire_connection().await?;
        let block_args = BlockArgs::pending(&mut connection).await?;
        drop(connection);
        let (mut tx, tx_hash) = self
            .state
            .parse_transaction_bytes(&tx_bytes.0, &block_args)?;
        tx.set_input(tx_bytes.0, tx_hash);

        let submit_output = self
            .state
            .tx_sender
            .submit_tx(tx, block_args)
            .await
            .map_err(|err| self.current_method().map_submit_err(err))?;
        Ok(TransactionDetailedResult {
            transaction_hash: tx_hash,
            storage_logs: submit_output
                .write_logs
                .into_iter()
                .map(Into::into)
                .collect(),
            events: submit_output
                .events
                .into_iter()
                .map(|event| map_event(event, tx_hash))
                .collect(),
        })
    }
}

fn map_event(vm_event: VmEvent, tx_hash: H256) -> api::Log {
    api::Log {
        address: vm_event.address,
        topics: vm_event.indexed_topics,
        data: web3::Bytes::from(vm_event.value),
        block_hash: None,
        block_number: None,
        l1_batch_number: Some(U64::from(vm_event.location.0 .0)),
        transaction_hash: Some(tx_hash),
        transaction_index: Some(web3::Index::from(vm_event.location.1)),
        log_index: None,
        transaction_log_index: None,
        log_type: None,
        removed: Some(false),
        block_timestamp: None,
    }
}
