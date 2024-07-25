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
