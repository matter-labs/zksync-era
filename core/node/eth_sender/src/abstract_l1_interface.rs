use std::fmt;

use async_trait::async_trait;
use vise::{EncodeLabelSet, EncodeLabelValue};
use zksync_eth_client::{
    BoundEthInterface, EnrichedClientResult, EthInterface, ExecutedTxStatus, FailureInfo, Options,
    RawTransactionBytes, SignedCallResult,
};
#[cfg(test)]
use zksync_types::web3;
use zksync_types::{
    eth_sender::{EthTx, EthTxBlobSidecar},
    web3::{BlockId, BlockNumber},
    Address, L1BlockNumber, Nonce, EIP_1559_TX_TYPE, EIP_4844_TX_TYPE, EIP_712_TX_TYPE, H256, U256,
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
    Gateway,
}

#[async_trait]
pub(super) trait AbstractL1Interface: 'static + Sync + Send + fmt::Debug {
    fn supported_operator_types(&self) -> Vec<OperatorType>;

    async fn failure_reason(
        &self,
        tx_hash: H256,
        operator_type: OperatorType,
    ) -> Option<FailureInfo>;

    #[cfg(test)]
    async fn get_tx(
        &self,
        tx_hash: H256,
        operator_type: OperatorType,
    ) -> EnrichedClientResult<Option<web3::Transaction>>;

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

    fn get_operator_account(&self, operator_type: OperatorType) -> Address;

    async fn get_operator_nonce(
        &self,
        block_numbers: L1BlockNumbers,
        operator_type: OperatorType,
    ) -> Result<Option<OperatorNonce>, EthSenderError>;

    #[allow(clippy::too_many_arguments)]
    async fn sign_tx(
        &self,
        tx: &EthTx,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
        blob_gas_price: Option<U256>,
        max_aggregated_tx_gas: U256,
        operator_type: OperatorType,
        pubdata_limit: Option<U256>,
    ) -> SignedCallResult;

    async fn get_l1_block_numbers(
        &self,
        operator_type: OperatorType,
    ) -> Result<L1BlockNumbers, EthSenderError>;
}

#[derive(Debug)]
pub(super) struct RealL1Interface {
    pub ethereum_client: Option<Box<dyn BoundEthInterface>>,
    pub ethereum_client_blobs: Option<Box<dyn BoundEthInterface>>,
    pub sl_client: Option<Box<dyn BoundEthInterface>>,
    pub wait_confirmations: Option<u64>,
}

impl RealL1Interface {
    fn query_client(&self, operator_type: OperatorType) -> &dyn EthInterface {
        match operator_type {
            OperatorType::NonBlob => self.ethereum_client.as_deref().unwrap().as_ref(),
            OperatorType::Blob => self.ethereum_client_blobs.as_deref().unwrap().as_ref(),
            OperatorType::Gateway => self.sl_client.as_deref().unwrap().as_ref(),
        }
    }

    fn bound_query_client(&self, operator_type: OperatorType) -> &dyn BoundEthInterface {
        match operator_type {
            OperatorType::NonBlob => self.ethereum_client.as_deref().unwrap(),
            OperatorType::Blob => self.ethereum_client_blobs.as_deref().unwrap(),
            OperatorType::Gateway => self.sl_client.as_deref().unwrap(),
        }
    }
}

#[async_trait]
impl AbstractL1Interface for RealL1Interface {
    fn supported_operator_types(&self) -> Vec<OperatorType> {
        if self.sl_client.is_some() {
            return vec![OperatorType::Gateway];
        }

        let mut result = vec![];
        if self.ethereum_client_blobs.is_some() {
            result.push(OperatorType::Blob)
        }
        if self.ethereum_client.is_some() {
            result.push(OperatorType::NonBlob);
        }
        result
    }

    async fn failure_reason(
        &self,
        tx_hash: H256,
        operator_type: OperatorType,
    ) -> Option<FailureInfo> {
        self.query_client(operator_type)
            .failure_reason(tx_hash)
            .await
            .expect(
                "Tx is already failed, it's safe to fail here and apply the status on the next run",
            )
    }

    #[cfg(test)]
    async fn get_tx(
        &self,
        tx_hash: H256,
        operator_type: OperatorType,
    ) -> EnrichedClientResult<Option<web3::Transaction>> {
        self.query_client(operator_type).get_tx(tx_hash).await
    }

    async fn get_tx_status(
        &self,
        tx_hash: H256,
        operator_type: OperatorType,
    ) -> Result<Option<ExecutedTxStatus>, EthSenderError> {
        self.query_client(operator_type)
            .get_tx_status(tx_hash)
            .await
            .map_err(Into::into)
    }

    async fn send_raw_tx(
        &self,
        tx_bytes: RawTransactionBytes,
        operator_type: OperatorType,
    ) -> EnrichedClientResult<H256> {
        self.query_client(operator_type).send_raw_tx(tx_bytes).await
    }

    fn get_operator_account(&self, operator_type: OperatorType) -> Address {
        self.bound_query_client(operator_type).sender_account()
    }

    async fn get_operator_nonce(
        &self,
        block_numbers: L1BlockNumbers,
        operator_type: OperatorType,
    ) -> Result<Option<OperatorNonce>, EthSenderError> {
        let finalized = self
            .bound_query_client(operator_type)
            .nonce_at(block_numbers.finalized.0.into())
            .await?
            .as_u32()
            .into();

        let latest = self
            .bound_query_client(operator_type)
            .nonce_at(block_numbers.latest.0.into())
            .await?
            .as_u32()
            .into();
        Ok(Some(OperatorNonce { finalized, latest }))
    }

    async fn sign_tx(
        &self,
        tx: &EthTx,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
        blob_gas_price: Option<U256>,
        gas: U256,
        operator_type: OperatorType,
        max_gas_per_pubdata: Option<U256>,
    ) -> SignedCallResult {
        self.bound_query_client(operator_type)
            .sign_prepared_tx_for_addr(
                tx.raw_tx.clone(),
                tx.contract_address,
                Options::with(|opt| {
                    opt.gas = Some(gas);
                    opt.max_fee_per_gas = Some(U256::from(base_fee_per_gas + priority_fee_per_gas));
                    opt.max_priority_fee_per_gas = Some(U256::from(priority_fee_per_gas));
                    opt.nonce = Some(tx.nonce.0.into());
                    if let Some(max_gas_per_pubdata) = max_gas_per_pubdata {
                        assert!(
                            tx.is_gateway,
                            "Max gas per pubdata must be used only for gateway transaction"
                        );
                        opt.max_gas_per_pubdata = Some(max_gas_per_pubdata);
                        opt.transaction_type = Some(EIP_712_TX_TYPE.into());
                    } else {
                        opt.transaction_type = Some(EIP_1559_TX_TYPE.into());
                    }
                    if tx.blob_sidecar.is_some() {
                        opt.transaction_type = Some(EIP_4844_TX_TYPE.into());
                        opt.max_fee_per_blob_gas = blob_gas_price;
                        opt.blob_versioned_hashes = tx.blob_sidecar.as_ref().map(|s| match s {
                            EthTxBlobSidecar::EthTxBlobSidecarV1(s) => s
                                .blobs
                                .iter()
                                .map(|blob| H256::from_slice(&blob.versioned_hash))
                                .collect(),
                        });
                    }
                }),
            )
            .await
            .expect("Failed to sign transaction")
    }

    async fn get_l1_block_numbers(
        &self,
        operator_type: OperatorType,
    ) -> Result<L1BlockNumbers, EthSenderError> {
        let (finalized, safe) = if let Some(confirmations) = self.wait_confirmations {
            let latest_block_number: u64 = self
                .query_client(operator_type)
                .block_number()
                .await?
                .as_u64();

            let finalized = (latest_block_number.saturating_sub(confirmations) as u32).into();
            (finalized, finalized)
        } else {
            let finalized = self
                .query_client(operator_type)
                .block(BlockId::Number(BlockNumber::Finalized))
                .await?
                .expect("Finalized block must be present on L1")
                .number
                .expect("Finalized block must contain number")
                .as_u32()
                .into();

            let safe = self
                .query_client(operator_type)
                .block(BlockId::Number(BlockNumber::Safe))
                .await?
                .expect("Safe block must be present on L1")
                .number
                .expect("Safe block must contain number")
                .as_u32()
                .into();
            (finalized, safe)
        };

        let latest = self
            .query_client(operator_type)
            .block_number()
            .await?
            .as_u32()
            .into();

        Ok(L1BlockNumbers {
            finalized,
            latest,
            safe,
        })
    }
}
