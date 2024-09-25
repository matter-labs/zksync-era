use std::num::NonZeroU64;

use fraction::Fraction;

/// Using the base token price and eth price, calculate the fraction of the base token to eth.
pub fn get_fraction(ratio_f64: f64) -> (NonZeroU64, NonZeroU64) {
    let rate_fraction = Fraction::from(ratio_f64);

    let numerator = NonZeroU64::new(*rate_fraction.numer().expect("numerator is empty"))
        .expect("numerator is zero");
    let denominator = NonZeroU64::new(*rate_fraction.denom().expect("denominator is empty"))
        .expect("denominator is zero");

    (numerator, denominator)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn assert_get_fraction_value(f: f64, num: u64, denum: u64) {
        assert_eq!(
            get_fraction(f),
            (
                NonZeroU64::try_from(num).unwrap(),
                NonZeroU64::try_from(denum).unwrap()
            )
        );
    }

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
    }
}
