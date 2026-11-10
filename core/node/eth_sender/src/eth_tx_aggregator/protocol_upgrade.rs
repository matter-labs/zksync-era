use std::time::{SystemTime, UNIX_EPOCH};

use zksync_dal::{Connection, Core, CoreDal};
use zksync_eth_client::CallFunctionArgs;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    eth_sender::EthTx,
    ethabi::{self, Token},
    protocol_version::ProtocolSemanticVersion,
    web3::{keccak256, CallRequest},
    Address, H256, U256,
};

use super::EthTxAggregator;
use crate::{aggregator::OperationSkippingRestrictions, EthSenderError};

impl EthTxAggregator {
    /// Saves an `upgradeChainFromVersion` tx to the validator timelock once the scheduled upgrade
    /// is active on L1 and all batches that depend on the current version are proven.
    /// Returns `true` if the tx was saved.
    pub(super) async fn handle_protocol_upgrade(
        &mut self,
        storage: &mut Connection<'_, Core>,
        chain_protocol_version: ProtocolSemanticVersion,
        timelock_contract_address: Address,
        is_gateway: bool,
        op_restrictions: &mut OperationSkippingRestrictions,
    ) -> Result<bool, EthSenderError> {
        if storage
            .eth_sender_dal()
            .has_unconfirmed_txs_of_type(AggregatedActionType::ProtocolUpgrade)
            .await?
        {
            let reason = Some("protocol upgrade tx is not confirmed yet");
            op_restrictions.commit_restriction = reason;
            op_restrictions.prove_restriction = reason;
            op_restrictions.execute_restriction = reason;
            op_restrictions.precommit_restriction = reason;
            return Ok(false);
        }

        // Operator-driven upgrades are only supported from v31 and for chains settling on L1.
        if is_gateway || chain_protocol_version.minor.is_pre_medium_interop() {
            return Ok(false);
        }

        let Some((new_protocol_version, upgrade_cut_data)) = storage
            .protocol_versions_dal()
            .next_version_with_upgrade_cut_data(chain_protocol_version)
            .await?
        else {
            return Ok(false);
        };

        let upgrade_timestamp = self.upgrade_timestamp(chain_protocol_version).await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("incorrect system time")
            .as_secs();
        if upgrade_timestamp.is_zero() || upgrade_timestamp > U256::from(now) {
            return Ok(false);
        }

        let Some(upgrade_cut_data) = upgrade_cut_data else {
            tracing::warn!(
                "No diamond cut is stored for the upgrade {chain_protocol_version} -> \
                 {new_protocol_version}, it has to be executed by the chain admin"
            );
            return Ok(false);
        };

        if !self
            .is_ready_for_protocol_upgrade(storage, chain_protocol_version, new_protocol_version)
            .await?
        {
            tracing::info!(
                "Protocol upgrade {chain_protocol_version} -> {new_protocol_version} is waiting \
                 for batches to be proven"
            );
            return Ok(false);
        }

        let expected_cut_hash = self.upgrade_cut_hash(chain_protocol_version).await?;
        let cut_hash = H256(keccak256(&upgrade_cut_data));
        if cut_hash != expected_cut_hash {
            tracing::warn!(
                "Stored diamond cut for the upgrade {chain_protocol_version} -> \
                 {new_protocol_version} has hash {cut_hash:?}, while L1 expects \
                 {expected_cut_hash:?}"
            );
            return Ok(false);
        }

        let calldata = match self
            .encode_upgrade_chain_from_version(chain_protocol_version, &upgrade_cut_data)
        {
            Ok(calldata) => calldata,
            Err(err) => {
                tracing::warn!(
                    "Failed to decode the diamond cut for the upgrade {chain_protocol_version} -> \
                     {new_protocol_version}: {err}"
                );
                return Ok(false);
            }
        };
        let request = CallRequest {
            from: Some(self.eth_client.sender_account()),
            to: Some(timelock_contract_address),
            gas: Some(self.config.max_aggregated_tx_gas.into()),
            data: Some(calldata.clone().into()),
            ..CallRequest::default()
        };
        if let Err(err) = (*self.eth_client)
            .as_ref()
            .call_contract_function(request, None)
            .await
        {
            tracing::warn!(
                "Protocol upgrade {chain_protocol_version} -> {new_protocol_version} fails in \
                 simulation: {err}"
            );
            return Ok(false);
        }

        let tx = self
            .save_protocol_upgrade_tx(storage, calldata, timelock_contract_address)
            .await?;
        tracing::info!(
            "eth_tx with ID {} for protocol upgrade {chain_protocol_version} -> \
             {new_protocol_version} was saved",
            tx.id
        );
        Ok(true)
    }

    /// For minor upgrades every batch of the previous version has to be proven. Patch upgrades
    /// only replace the verifier, so the next batch to prove has to have a proof for the new
    /// version: a proof for the previous one is rejected after the upgrade and never redone.
    async fn is_ready_for_protocol_upgrade(
        &self,
        storage: &mut Connection<'_, Core>,
        chain_protocol_version: ProtocolSemanticVersion,
        new_protocol_version: ProtocolSemanticVersion,
    ) -> Result<bool, EthSenderError> {
        let last_proven = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_proven_on_eth()
            .await?
            .unwrap_or_default();

        if new_protocol_version.minor > chain_protocol_version.minor {
            let Some(last_sealed) = storage.blocks_dal().get_sealed_l1_batch_number().await? else {
                return Ok(false);
            };
            let last_sealed_version = storage
                .blocks_dal()
                .get_batch_protocol_version_id(last_sealed)
                .await?;
            if last_sealed_version < Some(new_protocol_version.minor) {
                return Ok(false);
            }
            let last_previous_version_batch = storage
                .blocks_dal()
                .get_last_sealed_l1_batch_with_protocol_version_below(new_protocol_version.minor)
                .await?;
            return Ok(last_previous_version_batch.is_none_or(|number| number <= last_proven));
        }

        let last_with_prove_tx = storage
            .blocks_dal()
            .get_last_l1_batch_with_prove_tx()
            .await?;
        if last_with_prove_tx > last_proven {
            return Ok(false);
        }
        Ok(self
            .aggregator
            .has_proof_for_version(last_with_prove_tx + 1, new_protocol_version)
            .await)
    }

    async fn upgrade_timestamp(
        &self,
        chain_protocol_version: ProtocolSemanticVersion,
    ) -> Result<U256, EthSenderError> {
        let server_notifier: Address = CallFunctionArgs::new("serverNotifierAddress", ())
            .for_contract(
                self.state_transition_manager_address,
                &self.functions.state_transition_manager_contract,
            )
            .call((*self.eth_client).as_ref())
            .await?;
        Ok(CallFunctionArgs::new(
            "protocolVersionToUpgradeTimestamp",
            (
                U256::from(self.rollup_chain_id.as_u64()),
                chain_protocol_version.pack(),
            ),
        )
        .for_contract(server_notifier, &self.functions.server_notifier_contract)
        .call((*self.eth_client).as_ref())
        .await?)
    }

    async fn upgrade_cut_hash(
        &self,
        chain_protocol_version: ProtocolSemanticVersion,
    ) -> Result<H256, EthSenderError> {
        Ok(
            CallFunctionArgs::new("upgradeCutHash", chain_protocol_version.pack())
                .for_contract(
                    self.state_transition_manager_address,
                    &self.functions.state_transition_manager_contract,
                )
                .call((*self.eth_client).as_ref())
                .await?,
        )
    }

    fn encode_upgrade_chain_from_version(
        &self,
        chain_protocol_version: ProtocolSemanticVersion,
        upgrade_cut_data: &[u8],
    ) -> Result<Vec<u8>, ethabi::Error> {
        let function = &self.functions.upgrade_chain_from_version;
        let diamond_cut =
            ethabi::decode(&[function.inputs[2].kind.clone()], upgrade_cut_data)?.remove(0);
        Ok(function
            .encode_input(&[
                Token::Address(self.state_transition_chain_contract),
                Token::Uint(chain_protocol_version.pack()),
                diamond_cut,
            ])
            .expect("failed to encode upgradeChainFromVersion"))
    }

    async fn save_protocol_upgrade_tx(
        &self,
        storage: &mut Connection<'_, Core>,
        calldata: Vec<u8>,
        timelock_contract_address: Address,
    ) -> Result<EthTx, EthSenderError> {
        let mut transaction = storage.start_transaction().await?;
        let sender_addr = self.eth_client.sender_account();
        let nonce = self
            .get_next_nonce(&mut transaction, sender_addr, false)
            .await?;
        let mut eth_tx = transaction
            .eth_sender_dal()
            .save_eth_tx(
                nonce,
                calldata,
                AggregatedActionType::ProtocolUpgrade,
                timelock_contract_address,
                None,
                Some(sender_addr),
                None,
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
        transaction.commit().await?;
        Ok(eth_tx)
    }
}

#[cfg(test)]
mod tests;
