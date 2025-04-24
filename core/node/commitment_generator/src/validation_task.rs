use std::time::Duration;

use tokio::sync::watch;
use zksync_eth_client::{CallFunctionArgs, ClientError, ContractCallError, EthInterface};
use zksync_types::{commitment::L1BatchCommitmentMode, Address};

/// Managed task that asynchronously validates that the commitment mode (rollup or validium) from the node config
/// matches the mode in the L1 diamond proxy contract.
#[derive(Debug)]
pub struct L1BatchCommitmentModeValidationTask {
    diamond_proxy_address: Address,
    expected_mode: L1BatchCommitmentMode,
    eth_client: Box<dyn EthInterface>,
    retry_interval: Duration,
    exit_on_success: bool,
}

impl L1BatchCommitmentModeValidationTask {
    const DEFAULT_RETRY_INTERVAL: Duration = Duration::from_secs(5);

    /// Creates a new task with the specified params and Ethereum client.
    pub fn new(
        diamond_proxy_address: Address,
        expected_mode: L1BatchCommitmentMode,
        eth_client: Box<dyn EthInterface>,
    ) -> Self {
        Self {
            diamond_proxy_address,
            expected_mode,
            eth_client,
            retry_interval: Self::DEFAULT_RETRY_INTERVAL,
            exit_on_success: false,
        }
    }

    /// Makes the task exit after the commitment mode was successfully verified. By default, the task
    /// will only exit on error or after getting a stop signal.
    pub fn exit_on_success(mut self) -> Self {
        self.exit_on_success = true;
        self
    }

    async fn validate_commitment_mode(self) -> anyhow::Result<()> {
        let expected_mode = self.expected_mode;
        let diamond_proxy_address = self.diamond_proxy_address;
        loop {
            let result =
                Self::get_pubdata_pricing_mode(diamond_proxy_address, self.eth_client.as_ref())
                    .await;
            match result {
                Ok(mode) => {
                    anyhow::ensure!(
                        mode == self.expected_mode,
                        "Configured L1 batch commitment mode ({expected_mode:?}) does not match the commitment mode \
                         used on L1 contract {diamond_proxy_address:?} ({mode:?})"
                    );
                    tracing::info!(
                        "Checked that the configured L1 batch commitment mode ({expected_mode:?}) matches the commitment mode \
                         used on L1 contract {diamond_proxy_address:?}"
                    );
                    return Ok(());
                }

                // Getters contract does not support `getPubdataPricingMode` method.
                // This case is accepted for backwards compatibility with older contracts, but emits a
                // warning in case the wrong contract address was passed by the caller.
                Err(ContractCallError::EthereumGateway(err))
                    if matches!(err.as_ref(), ClientError::Call(_)) =>
                {
                    tracing::warn!("Contract {diamond_proxy_address:?} does not support getPubdataPricingMode method: {err}");
                    return Ok(());
                }

                Err(ContractCallError::EthereumGateway(err)) if err.is_retryable() => {
                    tracing::warn!(
                        "Transient error validating commitment mode, will retry after {:?}: {err}",
                        self.retry_interval
                    );
                    tokio::time::sleep(self.retry_interval).await;
                }

                Err(err) => {
                    tracing::error!("Fatal error validating commitment mode: {err}");
                    return Err(err.into());
                }
            }
        }
    }

    async fn get_pubdata_pricing_mode(
        diamond_proxy_address: Address,
        eth_client: &dyn EthInterface,
    ) -> Result<L1BatchCommitmentMode, ContractCallError> {
        CallFunctionArgs::new("getPubdataPricingMode", ())
            .for_contract(
                diamond_proxy_address,
                &zksync_contracts::hyperchain_contract(),
            )
            .call(eth_client)
            .await
    }

    /// Runs this task. The task will exit on error (and on success if `exit_on_success` is set),
    /// or when a stop signal is received.
    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let exit_on_success = self.exit_on_success;
        let validation = self.validate_commitment_mode();
        tokio::select! {
            result = validation => {
                if exit_on_success || result.is_err() {
                    return result;
                }
                stop_receiver.changed().await.ok();
                Ok(())
            },
            _ = stop_receiver.changed() => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{mem, sync::Mutex};

    use zksync_eth_client::clients::MockSettlementLayer;
    use zksync_types::{ethabi, U256};
    use zksync_web3_decl::{
        client::{MockClient, L1},
        jsonrpsee::{core::BoxError, types::ErrorObject},
    };

    use super::*;

    fn mock_ethereum(token: ethabi::Token, err: Option<ClientError>) -> MockClient<L1> {
        let err_mutex = Mutex::new(err);
        MockSettlementLayer::builder()
            .with_fallible_call_handler(move |_, _| {
                let err = mem::take(&mut *err_mutex.lock().unwrap());
                if let Some(err) = err {
                    Err(err)
                } else {
                    Ok(token.clone())
                }
            })
            .build()
            .into_client()
    }

    fn mock_ethereum_with_legacy_contract() -> MockClient<L1> {
        let err = ClientError::Call(ErrorObject::owned(3, "execution reverted: F", None::<()>));
        mock_ethereum(ethabi::Token::Uint(U256::zero()), Some(err))
    }

    fn mock_ethereum_with_transport_error() -> MockClient<L1> {
        let err = ClientError::Transport(BoxError::from(anyhow::anyhow!("unreachable")));
        mock_ethereum(ethabi::Token::Uint(U256::zero()), Some(err))
    }

    fn mock_ethereum_with_rollup_contract() -> MockClient<L1> {
        mock_ethereum(ethabi::Token::Uint(U256::zero()), None)
    }

    fn mock_ethereum_with_validium_contract() -> MockClient<L1> {
        mock_ethereum(ethabi::Token::Uint(U256::one()), None)
    }

    fn commitment_task(
        expected_mode: L1BatchCommitmentMode,
        eth_client: MockClient<L1>,
    ) -> L1BatchCommitmentModeValidationTask {
        let diamond_proxy_address = Address::repeat_byte(0x01);
        L1BatchCommitmentModeValidationTask {
            diamond_proxy_address,
            expected_mode,
            eth_client: Box::new(eth_client),
            exit_on_success: true,
            retry_interval: Duration::ZERO,
        }
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_succeeds_when_both_match() {
        let task = commitment_task(
            L1BatchCommitmentMode::Rollup,
            mock_ethereum_with_rollup_contract(),
        );
        task.validate_commitment_mode().await.unwrap();

        let task = commitment_task(
            L1BatchCommitmentMode::Validium,
            mock_ethereum_with_validium_contract(),
        );
        task.validate_commitment_mode().await.unwrap();
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_succeeds_on_legacy_contracts() {
        let task = commitment_task(
            L1BatchCommitmentMode::Rollup,
            mock_ethereum_with_legacy_contract(),
        );
        task.validate_commitment_mode().await.unwrap();

        let task = commitment_task(
            L1BatchCommitmentMode::Validium,
            mock_ethereum_with_legacy_contract(),
        );
        task.validate_commitment_mode().await.unwrap();
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_mismatch() {
        let task = commitment_task(
            L1BatchCommitmentMode::Validium,
            mock_ethereum_with_rollup_contract(),
        );
        let err = task
            .validate_commitment_mode()
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("commitment mode"), "{err}");

        let task = commitment_task(
            L1BatchCommitmentMode::Rollup,
            mock_ethereum_with_validium_contract(),
        );
        let err = task
            .validate_commitment_mode()
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("commitment mode"), "{err}");
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_recovers_from_request_failure() {
        let task = commitment_task(
            L1BatchCommitmentMode::Rollup,
            mock_ethereum_with_transport_error(),
        );
        task.validate_commitment_mode().await.unwrap();
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_parse_error() {
        let task = commitment_task(
            L1BatchCommitmentMode::Rollup,
            mock_ethereum(ethabi::Token::String("what".into()), None),
        );
        let err = task
            .validate_commitment_mode()
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("L1BatchCommitDataGeneratorMode::from_tokens"),
            "{err}",
        );
    }
}
