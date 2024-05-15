//! Miscellaneous utils used by multiple components.

use zksync_config::configs::chain::L1BatchCommitDataGeneratorMode;
use zksync_eth_client::{CallFunctionArgs, ClientError, Error as EthClientError, EthInterface};
use zksync_types::Address;

async fn get_pubdata_pricing_mode(
    diamond_proxy_address: Address,
    eth_client: &dyn EthInterface,
) -> Result<L1BatchCommitDataGeneratorMode, EthClientError> {
    CallFunctionArgs::new("getPubdataPricingMode", ())
        .for_contract(
            diamond_proxy_address,
            &zksync_contracts::hyperchain_contract(),
        )
        .call(eth_client)
        .await
}

pub async fn ensure_l1_batch_commit_data_generation_mode(
    selected_l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
    diamond_proxy_address: Address,
    eth_client: &dyn EthInterface,
) -> anyhow::Result<()> {
    match get_pubdata_pricing_mode(diamond_proxy_address, eth_client).await {
        // Getters contract support getPubdataPricingMode method
        Ok(l1_contract_batch_commitment_mode) => {
            anyhow::ensure!(
                l1_contract_batch_commitment_mode == selected_l1_batch_commit_data_generator_mode,
                "The selected L1BatchCommitDataGeneratorMode ({:?}) does not match the commitment mode used on L1 contract ({:?})",
                selected_l1_batch_commit_data_generator_mode,
                l1_contract_batch_commitment_mode
            );

            Ok(())
        }
        // Getters contract does not support getPubdataPricingMode method.
        // This case is accepted for backwards compatibility with older contracts, but emits a
        // warning in case the wrong contract address was passed by the caller.
        Err(EthClientError::EthereumGateway(err))
            if matches!(err.as_ref(), ClientError::Call(_)) =>
        {
            tracing::warn!("Getters contract does not support getPubdataPricingMode method: {err}");
            Ok(())
        }
        Err(err) => anyhow::bail!(err),
    }
}

#[cfg(test)]
mod tests {
    use std::{mem, sync::Mutex};

    use jsonrpsee::types::ErrorObject;
    use zksync_eth_client::{clients::MockEthereum, ClientError};
    use zksync_types::{ethabi, U256};
    use zksync_web3_decl::error::EnrichedClientError;

    use super::*;

    fn mock_ethereum(token: ethabi::Token, err: Option<EthClientError>) -> MockEthereum {
        let err_mutex = Mutex::new(err);
        MockEthereum::default().with_fallible_call_handler(move |_, _| {
            let err = mem::take(&mut *err_mutex.lock().unwrap());
            if let Some(err) = err {
                Err(err)
            } else {
                Ok(token.clone())
            }
        })
    }

    fn mock_ethereum_with_legacy_contract() -> MockEthereum {
        let err = ClientError::Call(ErrorObject::owned(3, "execution reverted: F", None::<()>));
        let err = EthClientError::EthereumGateway(EnrichedClientError::new(err, "call"));
        mock_ethereum(ethabi::Token::Uint(U256::zero()), Some(err))
    }

    fn mock_ethereum_with_transport_error() -> MockEthereum {
        let err = ClientError::Transport(anyhow::anyhow!("unreachable"));
        let err = EthClientError::EthereumGateway(EnrichedClientError::new(err, "call"));
        mock_ethereum(ethabi::Token::Uint(U256::zero()), Some(err))
    }

    fn mock_ethereum_with_rollup_contract() -> MockEthereum {
        mock_ethereum(ethabi::Token::Uint(U256::zero()), None)
    }

    fn mock_ethereum_with_validium_contract() -> MockEthereum {
        mock_ethereum(ethabi::Token::Uint(U256::one()), None)
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_succeeds_when_both_match() {
        let addr = Address::repeat_byte(0x01);
        ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Rollup,
            addr,
            &mock_ethereum_with_rollup_contract(),
        )
        .await
        .unwrap();

        ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Validium,
            addr,
            &mock_ethereum_with_validium_contract(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_succeeds_on_legacy_contracts() {
        let addr = Address::repeat_byte(0x01);
        ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Rollup,
            addr,
            &mock_ethereum_with_legacy_contract(),
        )
        .await
        .unwrap();

        ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Validium,
            addr,
            &mock_ethereum_with_legacy_contract(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_mismatch() {
        let addr = Address::repeat_byte(0x01);
        let err = ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Validium,
            addr,
            &mock_ethereum_with_rollup_contract(),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("commitment mode"), "{err}");

        let err = ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Rollup,
            addr,
            &mock_ethereum_with_validium_contract(),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("commitment mode"), "{err}");
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_request_failure() {
        let addr = Address::repeat_byte(0x01);
        let err = ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Rollup,
            addr,
            &mock_ethereum_with_transport_error(),
        )
        .await
        .unwrap_err();

        assert!(
            err.chain().any(|cause| cause.is::<ClientError>()),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn ensure_l1_batch_commit_data_generation_mode_fails_on_parse_error() {
        let addr = Address::repeat_byte(0x01);

        let err = ensure_l1_batch_commit_data_generation_mode(
            L1BatchCommitDataGeneratorMode::Rollup,
            addr,
            &mock_ethereum(ethabi::Token::String("what".into()), None),
        )
        .await
        .unwrap_err()
        .to_string();

        assert!(
            err.contains("L1BatchCommitDataGeneratorMode::from_tokens"),
            "{err}",
        );
    }
}
