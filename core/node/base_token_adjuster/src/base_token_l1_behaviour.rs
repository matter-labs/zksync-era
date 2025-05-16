use std::{
    cmp::max,
    ops::{Div, Mul},
    sync::Arc,
    time::Instant,
};

use anyhow::Context;
use bigdecimal::{BigDecimal, Zero};
use zksync_config::BaseTokenAdjusterConfig;
use zksync_eth_client::{BoundEthInterface, CallFunctionArgs, Options};
use zksync_node_fee_model::l1_gas_price::TxParamsProvider;
use zksync_types::{
    base_token_ratio::BaseTokenApiRatio,
    ethabi::{Contract, Token},
    web3::{contract::Tokenize, BlockNumber},
    Address, U256,
};

use crate::metrics::{OperationResult, OperationResultLabels, METRICS};

#[derive(Debug, Clone)]
pub struct UpdateOnL1Params {
    pub eth_client: Box<dyn BoundEthInterface>,
    pub gas_adjuster: Arc<dyn TxParamsProvider>,
    pub token_multiplier_setter_account_address: Address,
    pub chain_admin_contract: Contract,
    pub getters_facet_contract: Contract,
    pub diamond_proxy_contract_address: Address,
    pub chain_admin_contract_address: Option<Address>,
    pub config: BaseTokenAdjusterConfig,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum BaseTokenL1Behaviour {
    UpdateOnL1 {
        params: UpdateOnL1Params,
        last_persisted_l1_ratio: Option<BigDecimal>,
    },
    NoOp,
}

impl BaseTokenL1Behaviour {
    pub async fn update_l1(&mut self, new_ratio: BaseTokenApiRatio) -> anyhow::Result<()> {
        let (l1_params, last_persisted_l1_ratio) = match self {
            BaseTokenL1Behaviour::UpdateOnL1 {
                ref params,
                ref last_persisted_l1_ratio,
            } => (&params.clone(), last_persisted_l1_ratio),
            BaseTokenL1Behaviour::NoOp => return Ok(()),
        };

        let prev_ratio = if let Some(prev_ratio) = last_persisted_l1_ratio {
            prev_ratio.clone()
        } else {
            let prev_ratio = self.get_current_ratio_from_l1(l1_params).await?;
            self.update_last_persisted_l1_ratio(prev_ratio.clone());
            tracing::info!(
                "Fetched current base token ratio from the L1: {}",
                prev_ratio
            );
            prev_ratio
        };

        let current_ratio = BigDecimal::from(new_ratio.numerator.get())
            .div(BigDecimal::from(new_ratio.denominator.get()));
        let deviation = Self::compute_deviation(prev_ratio.clone(), current_ratio.clone());

        if deviation < BigDecimal::from(l1_params.config.l1_update_deviation_percentage) {
            tracing::debug!(
                "Skipping L1 update. current_ratio {}, previous_ratio {}, deviation {}",
                current_ratio,
                prev_ratio,
                deviation
            );
            return Ok(());
        }

        let max_attempts = l1_params.config.l1_tx_sending_max_attempts;
        let sleep_duration = l1_params.config.l1_tx_sending_sleep_duration();
        let mut prev_base_fee_per_gas: Option<u64> = None;
        let mut prev_priority_fee_per_gas: Option<u64> = None;
        let mut last_error = None;
        for attempt in 0..max_attempts {
            let (base_fee_per_gas, priority_fee_per_gas) =
                self.get_eth_fees(l1_params, prev_base_fee_per_gas, prev_priority_fee_per_gas);

            let start_time = Instant::now();
            let result = self
                .do_update_l1(l1_params, new_ratio, base_fee_per_gas, priority_fee_per_gas)
                .await;

            match result {
                Ok(x) => {
                    tracing::info!(
                        "Updated base token multiplier on L1: numerator {}, denominator {}, base_fee_per_gas {}, priority_fee_per_gas {}, deviation {}",
                        new_ratio.numerator.get(),
                        new_ratio.denominator.get(),
                        base_fee_per_gas,
                        priority_fee_per_gas,
                        deviation
                    );
                    METRICS
                        .l1_gas_used
                        .set(x.unwrap_or(U256::zero()).low_u128() as u64);
                    METRICS.l1_update_latency[&OperationResultLabels {
                        result: OperationResult::Success,
                    }]
                        .observe(start_time.elapsed());
                    self.update_last_persisted_l1_ratio(
                        BigDecimal::from(new_ratio.numerator.get())
                            .div(BigDecimal::from(new_ratio.denominator.get())),
                    );

                    return Ok(());
                }
                Err(err) => {
                    tracing::info!(
                        "Failed to update base token multiplier on L1, attempt {}, base_fee_per_gas {}, priority_fee_per_gas {}: {}",
                        attempt,
                        base_fee_per_gas,
                        priority_fee_per_gas,
                        err
                    );
                    METRICS.l1_update_latency[&OperationResultLabels {
                        result: OperationResult::Failure,
                    }]
                        .observe(start_time.elapsed());

                    tokio::time::sleep(sleep_duration).await;
                    prev_base_fee_per_gas = Some(base_fee_per_gas);
                    prev_priority_fee_per_gas = Some(priority_fee_per_gas);
                    last_error = Some(err)
                }
            }
        }

        let error_message = "Failed to update base token multiplier on L1";
        Err(last_error
            .map(|x| x.context(error_message))
            .unwrap_or_else(|| anyhow::anyhow!(error_message)))
    }

    fn update_last_persisted_l1_ratio(&mut self, new_ratio: BigDecimal) {
        match self {
            BaseTokenL1Behaviour::UpdateOnL1 {
                params: _,
                ref mut last_persisted_l1_ratio,
            } => *last_persisted_l1_ratio = Some(new_ratio),
            BaseTokenL1Behaviour::NoOp => {}
        };
    }

    // TODO(EVM-924): this logic supports only `ChainAdminOwnable`.
    async fn do_update_l1(
        &self,
        l1_params: &UpdateOnL1Params,
        api_ratio: BaseTokenApiRatio,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
    ) -> anyhow::Result<Option<U256>> {
        let fn_set_token_multiplier = l1_params
            .chain_admin_contract
            .function("setTokenMultiplier")
            .context("`setTokenMultiplier` function must be present in the ChainAdmin contract")?;

        let calldata = fn_set_token_multiplier
            .encode_input(
                &(
                    Token::Address(l1_params.diamond_proxy_contract_address),
                    Token::Uint(api_ratio.numerator.get().into()),
                    Token::Uint(api_ratio.denominator.get().into()),
                )
                    .into_tokens(),
            )
            .context("failed encoding `setTokenMultiplier` input")?;

        let nonce = (*l1_params.eth_client)
            .as_ref()
            .nonce_at_for_account(
                l1_params.token_multiplier_setter_account_address,
                BlockNumber::Latest,
            )
            .await
            .with_context(|| "failed getting transaction count")?
            .as_u64();

        let options = Options {
            gas: Some(U256::from(l1_params.config.max_tx_gas)),
            nonce: Some(U256::from(nonce)),
            max_fee_per_gas: Some(U256::from(base_fee_per_gas + priority_fee_per_gas)),
            max_priority_fee_per_gas: Some(U256::from(priority_fee_per_gas)),
            ..Default::default()
        };

        let signed_tx = l1_params
            .eth_client
            .sign_prepared_tx_for_addr(
                calldata,
                l1_params.chain_admin_contract_address.unwrap(),
                options,
            )
            .await
            .context("cannot sign a `setTokenMultiplier` transaction")?;

        let hash = (*l1_params.eth_client)
            .as_ref()
            .send_raw_tx(signed_tx.raw_tx)
            .await
            .context("failed sending `setTokenMultiplier` transaction")?;

        let max_attempts = l1_params.config.l1_receipt_checking_max_attempts;
        let sleep_duration = l1_params.config.l1_receipt_checking_sleep_duration();
        for _i in 0..max_attempts {
            let maybe_receipt = (*l1_params.eth_client)
                .as_ref()
                .tx_receipt(hash)
                .await
                .context("failed getting receipt for `setTokenMultiplier` transaction")?;
            if let Some(receipt) = maybe_receipt {
                if receipt.status == Some(1.into()) {
                    return Ok(receipt.gas_used);
                }
                let reason = (*l1_params.eth_client)
                    .as_ref()
                    .failure_reason(hash)
                    .await
                    .context("failed getting failure reason of `setTokenMultiplier` transaction")?;
                return Err(anyhow::Error::msg(format!(
                    "`setTokenMultiplier` transaction {:?} failed with status {:?}, reason: {:?}",
                    hex::encode(hash),
                    receipt.status,
                    reason
                )));
            } else {
                tokio::time::sleep(sleep_duration).await;
            }
        }

        Err(anyhow::Error::msg(format!(
            "Unable to retrieve `setTokenMultiplier` transaction status in {} attempts",
            max_attempts
        )))
    }

    async fn get_current_ratio_from_l1(
        &self,
        l1_params: &UpdateOnL1Params,
    ) -> anyhow::Result<BigDecimal> {
        let numerator: U256 = CallFunctionArgs::new("baseTokenGasPriceMultiplierNominator", ())
            .for_contract(
                l1_params.diamond_proxy_contract_address,
                &l1_params.getters_facet_contract,
            )
            .call((*l1_params.eth_client).as_ref())
            .await?;
        let denominator: U256 = CallFunctionArgs::new("baseTokenGasPriceMultiplierDenominator", ())
            .for_contract(
                l1_params.diamond_proxy_contract_address,
                &l1_params.getters_facet_contract,
            )
            .call((*l1_params.eth_client).as_ref())
            .await?;
        Ok(BigDecimal::from(numerator.as_u128()).div(BigDecimal::from(denominator.as_u128())))
    }

    fn get_eth_fees(
        &self,
        l1_params: &UpdateOnL1Params,
        prev_base_fee_per_gas: Option<u64>,
        prev_priority_fee_per_gas: Option<u64>,
    ) -> (u64, u64) {
        // Use get_blob_tx_base_fee here instead of get_base_fee to optimise for fast inclusion.
        // get_base_fee might cause the transaction to be stuck in the mempool for 10+ minutes.
        let mut base_fee_per_gas = l1_params.gas_adjuster.as_ref().get_blob_tx_base_fee();
        let mut priority_fee_per_gas = l1_params.gas_adjuster.as_ref().get_priority_fee();
        if let Some(x) = prev_priority_fee_per_gas {
            // Increase `priority_fee_per_gas` by at least 20% to prevent "replacement transaction under-priced" error.
            priority_fee_per_gas = max(priority_fee_per_gas, (x * 6) / 5 + 1);
        }

        if let Some(x) = prev_base_fee_per_gas {
            // same for base_fee_per_gas but 10%
            base_fee_per_gas = max(base_fee_per_gas, x + (x / 10) + 1);
        }

        // Extra check to prevent sending transaction with extremely high priority fee.
        if priority_fee_per_gas > l1_params.config.max_acceptable_priority_fee_in_gwei {
            panic!(
                "Extremely high value of priority_fee_per_gas is suggested: {}, while max acceptable is {}",
                priority_fee_per_gas,
                l1_params.config.max_acceptable_priority_fee_in_gwei
            );
        }

        (base_fee_per_gas, priority_fee_per_gas)
    }

    fn compute_deviation(prev: BigDecimal, next: BigDecimal) -> BigDecimal {
        if prev.eq(&BigDecimal::zero()) {
            return BigDecimal::from(100);
        }

        (prev.clone() - next.clone())
            .abs()
            .div(prev.clone())
            .mul(BigDecimal::from(100))
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Div;

    use bigdecimal::{BigDecimal, Zero};

    use crate::base_token_l1_behaviour::BaseTokenL1Behaviour;

    #[test]
    fn test_compute_deviation() {
        let prev_ratio = BigDecimal::from(4);
        let current_ratio = BigDecimal::from(5);
        let deviation =
            BaseTokenL1Behaviour::compute_deviation(prev_ratio.clone(), current_ratio.clone());
        assert_eq!(deviation, BigDecimal::from(25));

        let deviation = BaseTokenL1Behaviour::compute_deviation(current_ratio, prev_ratio);
        assert_eq!(deviation, BigDecimal::from(20));
    }

    #[test]
    fn test_compute_deviation_when_prev_is_zero() {
        let prev_ratio = BigDecimal::zero();
        let current_ratio = BigDecimal::from(1).div(BigDecimal::from(2));
        let deviation = BaseTokenL1Behaviour::compute_deviation(prev_ratio, current_ratio);
        assert_eq!(deviation, BigDecimal::from(100));
    }
}
