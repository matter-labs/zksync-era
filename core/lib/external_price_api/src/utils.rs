use std::num::NonZeroU64;

use chrono::Utc;
use fraction::GenericFraction;
use zksync_types::{base_token_ratio::BaseTokenApiRatio, fee_model::ConversionRatio};

/// Using the base token price and eth price, calculate the fraction of the base token to eth.
pub fn get_fraction(ratio_f64: f64) -> anyhow::Result<(NonZeroU64, NonZeroU64)> {
    // fraction::Fraction uses u64 under the hood and conversion from f64 mail fail
    // if the number is too small resulting in a big denominator. We use u128
    // and then bitshift the denominator and numerator to fit
    let rate_fraction = GenericFraction::<u128>::from(ratio_f64);
    if rate_fraction.sign() == Some(fraction::Sign::Minus) {
        return Err(anyhow::anyhow!("number is negative"));
    }

    // we need to potentially lower precision to fit into u64
    let denominator_before_division = rate_fraction
        .denom()
        .ok_or(anyhow::anyhow!("number is not rational"))?;

    // how many lowest bits we need to "loose" to fit into u64
    let bits_to_shift = 64_u32.saturating_sub(denominator_before_division.leading_zeros());

    let numerator = NonZeroU64::new(
        (*rate_fraction
            .numer()
            .ok_or(anyhow::anyhow!("number is not rational"))?
            >> bits_to_shift)
            .try_into()?,
    )
    .ok_or(anyhow::anyhow!("numerator is zero"))?;
    let denominator = NonZeroU64::new((denominator_before_division >> bits_to_shift).try_into()?)
        .ok_or(anyhow::anyhow!("denominator is zero"))?;

    Ok((numerator, denominator))
}

/// Take float price in ETH (1 BaseToken = X ETH) and convert it to BaseToken/ETH ratio (X BaseToken = 1 ETH)
pub fn eth_price_to_base_token_ratio(price: f64) -> anyhow::Result<BaseTokenApiRatio> {
    let (num_in_eth, denom_in_eth) = get_fraction(price)?;
    // take reciprocal of price as returned price is ETH/BaseToken and BaseToken/ETH is needed
    let (num_in_base, denom_in_base) = (denom_in_eth, num_in_eth);

    Ok(BaseTokenApiRatio {
        ratio: ConversionRatio {
            numerator: num_in_base,
            denominator: denom_in_base,
        },
        ratio_timestamp: Utc::now(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn assert_get_fraction_value(f: f64, num: u64, denum: u64) {
        assert_eq!(
            get_fraction(f).unwrap(),
            (
                NonZeroU64::try_from(num).unwrap(),
                NonZeroU64::try_from(denum).unwrap()
            )
        );
    }

    #[allow(clippy::approx_constant)]
    #[test]
    fn test_float_to_fraction_conversion_as_expected() {
        assert_get_fraction_value(1.0, 1, 1);
        assert_get_fraction_value(1337.0, 1337, 1);
        assert_get_fraction_value(0.1, 1, 10);
        assert_get_fraction_value(3.141, 3141, 1000);
        assert_get_fraction_value(1_000_000.0, 1_000_000, 1);
        assert_get_fraction_value(3123.47, 312347, 100);
        // below tests assume some not necessarily required behaviour of get_fraction
        assert_get_fraction_value(0.2, 1, 5);
        assert_get_fraction_value(0.5, 1, 2);
        assert_get_fraction_value(3.1415, 6283, 2000);
        assert_get_fraction_value(
            // very small value with high precision
            1.8615235072372934e-5,
            290863048005827,
            15625000000000000000,
        );
    }

    #[test]
    fn test_to_fraction_bad_inputs() {
        assert_eq!(
            get_fraction(0.0).expect_err("did not error").to_string(),
            "numerator is zero"
        );
        assert_eq!(
            get_fraction(-1.0).expect_err("did not error").to_string(),
            "number is negative"
        );
        assert_eq!(
            get_fraction(f64::NAN)
                .expect_err("did not error")
                .to_string(),
            "number is not rational"
        );
        assert_eq!(
            get_fraction(f64::INFINITY)
                .expect_err("did not error")
                .to_string(),
            "number is not rational"
        );
    }
}
