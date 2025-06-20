use crate::contracts;
use crate::zkstack_config::ZkstackConfig;
use alloy::consensus::{SidecarBuilder, SimpleCoder};
use alloy::network::{Ethereum, ReceiptResponse, TransactionBuilder, TransactionBuilder4844};
use alloy::providers::{DynProvider, Provider};
use alloy::rpc::types::TransactionRequest;
use std::sync::Arc;
use alloy::providers::ext::DebugApi;
use alloy::rpc::types::trace::geth::{CallConfig, GethDebugTracingOptions};
use tokio::sync::{mpsc, oneshot};
use zksync_types::commitment::{L1BatchWithMetadata, ZkosCommitment};
use zksync_types::hasher::keccak::KeccakHasher;
use zksync_types::l1::L1Tx;
use zksync_types::{Address, L2ChainId, H256};

/// Node component responsible for sending transactions to L1.
pub struct L1Sender {
    provider: DynProvider,
    l2_chain_id: L2ChainId,
    validator_timelock_addr: Address,
    command_receiver: mpsc::Receiver<Command>,
    last_committed_l1_batch: ZkosCommitment,
    last_proved_l1_batch: ZkosCommitment,
}

impl L1Sender {
    /// Initializes a new [`L1Sender`] that will send transaction using supplied provider. Assumes
    /// that zkstack config matches L1 configuration at the other end of provider.
    ///
    /// Resulting [`L1Sender`] is expected to be consumed by calling [`Self::run`]. Additionally,
    /// returns a cloneable handle that can be used to send requests to this instance of [`L1Sender`].
    pub fn new(
        zkstack_config: &ZkstackConfig,
        genesis_metadata: ZkosCommitment,
        provider: DynProvider,
    ) -> (Self, L1SenderHandle) {
        let (command_sender, command_receiver) = mpsc::channel(128);
        let this = Self {
            provider,
            l2_chain_id: zkstack_config.genesis.l2_chain_id,
            validator_timelock_addr: zkstack_config.contracts.l1.validator_timelock_addr,
            command_receiver,
            last_committed_l1_batch: genesis_metadata.clone(),
            last_proved_l1_batch: genesis_metadata,
        };
        let handle = L1SenderHandle { command_sender };
        (this, handle)
    }

    /// Runs L1 sender indefinitely thus processing requests received from any of the matching
    /// handles.
    pub async fn run(mut self) -> anyhow::Result<()> {
        while let Some(command) = self.command_receiver.recv().await {
            match command {
                Command::Commit(batch) => self.commit(batch).await?,
                // Command::Prove(batch, reply) => self.prove(batch, reply).await,
                // Command::Execute(batch, reply) => self.execute(batch, reply).await,
            }
        }

        tracing::trace!("channel has been closed; stopping L1 sender");
        Ok(())
    }
}

impl L1Sender {
    async fn commit(&mut self, commitment: ZkosCommitment) -> anyhow::Result<()> {
        // Create a blob sidecar with empty data
        let sidecar = SidecarBuilder::<SimpleCoder>::from_slice(&[]).build()?;

        let call = contracts::commit_batches_shared_bridge_call(
            self.l2_chain_id,
            &self.last_committed_l1_batch,
            &commitment,
        );

        let gas_price = self.provider.get_gas_price().await?;
        let eip1559_est = self.provider.estimate_eip1559_fees().await?;
        let tx = TransactionRequest::default()
            .with_to(self.validator_timelock_addr.0.into())
            .with_max_fee_per_blob_gas(gas_price)
            .with_max_fee_per_gas(eip1559_est.max_fee_per_gas)
            .with_max_priority_fee_per_gas(eip1559_est.max_priority_fee_per_gas)
            // Default value for `max_aggregated_tx_gas` from zksync-era, should always be enough
            .with_gas_limit(15000000)
            .with_call(&call)
            .with_blob_sidecar(sidecar);

        let pending_tx = self.provider.send_transaction(tx).await?;
        tracing::debug!(
            batch = commitment.batch_number,
            pending_tx_hash = ?pending_tx.tx_hash(),
            "block commit transaction sent to L1"
        );

        let receipt = pending_tx.get_receipt().await?;
        if receipt.status() {
            // We could also look at tx receipt's logs for a corresponding `BlockCommit` event but
            // not sure if this is 100% necessary.
            tracing::info!(
                batch = commitment.batch_number,
                tx_hash = ?receipt.transaction_hash,
                block_number = receipt.block_number.unwrap(),
                "block committed to L1",
            );

            // Commitment was successful, update last committed batch
            self.last_committed_l1_batch = commitment;
            
            Ok(())
        } else {
            tracing::error!(
                batch = commitment.batch_number,
                tx_hash = ?receipt.transaction_hash,
                block_number = receipt.block_number.unwrap(),
                "commit transaction failed"
            );
            if tracing::enabled!(tracing::Level::DEBUG) {
                let trace = self
                    .provider
                    .debug_trace_transaction(
                        receipt.transaction_hash,
                        GethDebugTracingOptions::call_tracer(CallConfig::default()),
                    )
                    .await?;
                let call_frame = trace.try_into_call_frame().expect("requested call tracer but received a different call frame type");
                // We print top-level call frame's output as it likely contains serialized custom
                // error pointing to the underlying problem (i.e. starts with the error's 4byte
                // signature).
                tracing::debug!(
                    ?call_frame.output,
                    ?call_frame.error,
                    ?call_frame.revert_reason,
                    "failed transaction's top-level call frame"
                );
            }
            anyhow::bail!(
                "commit transaction failed, see L1 transaction's trace for more details (tx_hash='{:?}')",
                receipt.transaction_hash
            );
        }
    }
}

/// A cheap cloneable handle to a [`L1Sender`] instance that can send requests.
#[derive(Clone, Debug)]
pub struct L1SenderHandle {
    command_sender: mpsc::Sender<Command>,
}

impl L1SenderHandle {
    /// Request [`L1Sender`] to send block's commitment to L1 asynchronously.
    pub async fn commit(&self, commitment: ZkosCommitment) -> anyhow::Result<()> {
        self.command_sender
            .send(Command::Commit(commitment))
            .await
            .map_err(|_| anyhow::anyhow!("failed to commit a block as L1 sender is dropped"))
    }
}

#[derive(Debug)]
enum Command {
    Commit(ZkosCommitment),
    // Prove(L1BatchWithMetadata),
    // Execute(L1BatchWithMetadata),
}
