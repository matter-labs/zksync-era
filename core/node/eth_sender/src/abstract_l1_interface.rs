use std::fmt;

use async_trait::async_trait;
use vise::{EncodeLabelSet, EncodeLabelValue};
use zksync_eth_client::{
    clients::{DynClient, L1},
    BoundEthInterface, EnrichedClientResult, EthInterface, ExecutedTxStatus, FailureInfo, Options,
    RawTransactionBytes, SignedCallResult,
};
#[cfg(test)]
use zksync_types::web3;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    eth_sender::{EthTx, EthTxBlobSidecar},
    web3::{BlockId, BlockNumber},
    Address, L1BlockNumber, Nonce, EIP_1559_TX_TYPE, EIP_4844_TX_TYPE, H256, U256,
};

use crate::EthSenderError;

#[derive(Debug, Clone, Copy)]
pub(crate) struct OperatorNonce {
    // Nonce on finalized block
    pub finalized: Nonce,
    // Nonce on latest block
    pub latest: Nonce,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct L1BlockNumbers {
    pub safe: L1BlockNumber,
    pub finalized: L1BlockNumber,
    pub latest: L1BlockNumber,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "type", rename_all = "snake_case")]
pub(crate) enum OperatorType {
    NonBlob,
    Blob,
}

#[async_trait]
pub(super) trait AbstractL1Interface: 'static + Sync + Send + fmt::Debug {
    async fn failure_reason(&self, tx_hash: H256) -> Option<FailureInfo>;

    #[cfg(test)]
    async fn get_tx(&self, tx_hash: H256) -> EnrichedClientResult<Option<web3::Transaction>>;

    async fn get_tx_status(
        &self,
        tx_hash: H256,
        operator_type: OperatorType,
    ) -> Result<Option<ExecutedTxStatus>, EthSenderError>;

    async fn send_raw_tx(
        &self,
        tx_bytes: RawTransactionBytes,
        operator_type: OperatorType,
    ) -> EnrichedClientResult<H256>;

    fn get_blobs_operator_account(&self) -> Option<Address>;

    async fn get_operator_nonce(
        &self,
        block_numbers: L1BlockNumbers,
        operator_type: OperatorType,
    ) -> Result<Option<OperatorNonce>, EthSenderError>;

    async fn sign_tx(
        &self,
        tx: &EthTx,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
        blob_gas_price: Option<U256>,
        max_aggregated_tx_gas: U256,
    ) -> SignedCallResult;

    async fn get_l1_block_numbers(&self) -> Result<L1BlockNumbers, EthSenderError>;

    fn ethereum_gateway(&self) -> &dyn BoundEthInterface<L1>;

    fn ethereum_gateway_blobs(&self) -> Option<&dyn BoundEthInterface<L1>>;
}

#[derive(Debug)]
pub(super) struct RealL1Interface {
    pub ethereum_gateway: Box<dyn BoundEthInterface<L1>>,
    pub ethereum_gateway_blobs: Option<Box<dyn BoundEthInterface<L1>>>,
    pub wait_confirmations: Option<u64>,
}

impl RealL1Interface {
    pub(crate) fn query_client(&self) -> &DynClient<L1> {
        self.ethereum_gateway().as_ref()
    }

    pub(crate) fn query_client_for_operator(&self, operator_type: OperatorType) -> &DynClient<L1> {
        if operator_type == OperatorType::Blob {
            self.ethereum_gateway_blobs().unwrap().as_ref()
        } else {
            self.ethereum_gateway().as_ref()
        }
    }
}

#[async_trait]
impl AbstractL1Interface for RealL1Interface {
    async fn failure_reason(&self, tx_hash: H256) -> Option<FailureInfo> {
        self.query_client().failure_reason(tx_hash).await.expect(
            "Tx is already failed, it's safe to fail here and apply the status on the next run",
        )
    }

    #[cfg(test)]
    async fn get_tx(&self, tx_hash: H256) -> EnrichedClientResult<Option<web3::Transaction>> {
        self.query_client().get_tx(tx_hash).await
    }

    async fn get_tx_status(
        &self,
        tx_hash: H256,
        operator_type: OperatorType,
    ) -> Result<Option<ExecutedTxStatus>, EthSenderError> {
        self.query_client_for_operator(operator_type)
            .get_tx_status(tx_hash)
            .await
            .map_err(Into::into)
    }

    async fn send_raw_tx(
        &self,
        tx_bytes: RawTransactionBytes,
        operator_type: OperatorType,
    ) -> EnrichedClientResult<H256> {
        self.query_client_for_operator(operator_type)
            .send_raw_tx(tx_bytes)
            .await
    }

    fn get_blobs_operator_account(&self) -> Option<Address> {
        self.ethereum_gateway_blobs()
            .as_ref()
            .map(|s| s.sender_account())
    }

    async fn get_operator_nonce(
        &self,
        block_numbers: L1BlockNumbers,
        operator_type: OperatorType,
    ) -> Result<Option<OperatorNonce>, EthSenderError> {
        let gateway = match operator_type {
            OperatorType::NonBlob => Some(self.ethereum_gateway()),
            OperatorType::Blob => self.ethereum_gateway_blobs(),
        };
        match gateway {
            None => Ok(None),
            Some(gateway) => {
                let finalized = gateway
                    .nonce_at(block_numbers.finalized.0.into())
                    .await?
                    .as_u32()
                    .into();

                let latest = gateway
                    .nonce_at(block_numbers.latest.0.into())
                    .await?
                    .as_u32()
                    .into();
                Ok(Some(OperatorNonce { finalized, latest }))
            }
        }
    }

    async fn sign_tx(
        &self,
        tx: &EthTx,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
        blob_gas_price: Option<U256>,
        max_aggregated_tx_gas: U256,
    ) -> SignedCallResult {
        // Chose the signing gateway. Use a custom one in case
        // the operator is in 4844 mode and the operation at hand is Commit.
        // then the optional gateway is used to send this transaction from a
        // custom sender account.
        let signing_gateway = if let Some(blobs_gateway) = self.ethereum_gateway_blobs() {
            if tx.tx_type == AggregatedActionType::Commit {
                blobs_gateway
            } else {
                self.ethereum_gateway()
            }
        } else {
            self.ethereum_gateway()
        };

        signing_gateway
            .sign_prepared_tx_for_addr(
                tx.raw_tx.clone(),
                tx.contract_address,
                Options::with(|opt| {
                    // TODO Calculate gas for every operation SMA-1436
                    opt.gas = Some(max_aggregated_tx_gas);
                    opt.max_fee_per_gas = Some(U256::from(base_fee_per_gas + priority_fee_per_gas));
                    opt.max_priority_fee_per_gas = Some(U256::from(priority_fee_per_gas));
                    opt.nonce = Some(tx.nonce.0.into());
                    opt.transaction_type = if tx.blob_sidecar.is_some() {
                        opt.max_fee_per_blob_gas = blob_gas_price;
                        Some(EIP_4844_TX_TYPE.into())
                    } else {
                        Some(EIP_1559_TX_TYPE.into())
                    };
                    opt.blob_versioned_hashes = tx.blob_sidecar.as_ref().map(|s| match s {
                        EthTxBlobSidecar::EthTxBlobSidecarV1(s) => s
                            .blobs
                            .iter()
                            .map(|blob| H256::from_slice(&blob.versioned_hash))
                            .collect(),
                    });
                }),
            )
            .await
            .expect("Failed to sign transaction")
    }

    async fn get_l1_block_numbers(&self) -> Result<L1BlockNumbers, EthSenderError> {
        let (finalized, safe) = if let Some(confirmations) = self.wait_confirmations {
            let latest_block_number = self.query_client().block_number().await?.as_u64();

            let finalized = (latest_block_number.saturating_sub(confirmations) as u32).into();
            (finalized, finalized)
        } else {
            let finalized = self
                .query_client()
                .block(BlockId::Number(BlockNumber::Finalized))
                .await?
                .expect("Finalized block must be present on L1")
                .number
                .expect("Finalized block must contain number")
                .as_u32()
                .into();

            let safe = self
                .query_client()
                .block(BlockId::Number(BlockNumber::Safe))
                .await?
                .expect("Safe block must be present on L1")
                .number
                .expect("Safe block must contain number")
                .as_u32()
                .into();
            (finalized, safe)
        };

        let latest = self.query_client().block_number().await?.as_u32().into();

        Ok(L1BlockNumbers {
            finalized,
            latest,
            safe,
        })
    }

    fn ethereum_gateway(&self) -> &dyn BoundEthInterface<L1> {
        self.ethereum_gateway.as_ref()
    }

    fn ethereum_gateway_blobs(&self) -> Option<&dyn BoundEthInterface<L1>> {
        self.ethereum_gateway_blobs.as_deref()
    }
}
