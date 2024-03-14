use std::{convert::TryInto, iter};

use kzg::{
    boojum::pairing::{bls12_381::G2Compressed, EncodedPoint},
    zkevm_circuits::{
        boojum::pairing::{
            bls12_381::{Fr, FrRepr, G1Compressed},
            ff::{Field as _, PrimeField as _},
            CurveAffine,
        },
        eip_4844::input::ELEMENTS_PER_4844_BLOCK,
    },
    KzgSettings,
};
use once_cell::sync::Lazy;

const FIRST_ROOT_OF_UNITY: FrRepr = FrRepr([
    0xe206da11a5d36306,
    0x0ad1347b378fbf96,
    0xfc3e8acfe0f8245f,
    0x564c0a11a0f704f4,
]);

fn bit_reverse_slice_indices<T>(array: &mut [T]) {
    assert_eq!(array.len(), ELEMENTS_PER_4844_BLOCK);
    for idx in 0..ELEMENTS_PER_4844_BLOCK {
        let reversed_idx = idx.reverse_bits() >> (usize::BITS - ELEMENTS_PER_4844_BLOCK.ilog2());
        if idx < reversed_idx {
            array.swap(idx, reversed_idx);
        }
    }
}

pub(super) static KZG_SETTINGS: Lazy<KzgSettings> = Lazy::new(|| {
    // Taken from the C KZG library: https://github.com/ethereum/c-kzg-4844/blob/main/src/trusted_setup.txt
    const TRUSTED_SETUP_STR: &str = include_str!("trusted_setup.txt");

    // Skip 2 first lines (number of G1 and G2 points).
    let mut lines = TRUSTED_SETUP_STR.lines().skip(2);

    let first_root_of_unity =
        Fr::from_repr(FIRST_ROOT_OF_UNITY).expect("invalid first root of unity");
    let mut roots_of_unity: Box<[_]> = iter::successors(Some(Fr::one()), |prev| {
        let mut next = first_root_of_unity;
        next.mul_assign(prev);
        Some(next)
    })
    .take(ELEMENTS_PER_4844_BLOCK)
    .collect();
    bit_reverse_slice_indices(&mut roots_of_unity);

    let lagrange_setup = lines.by_ref().take(ELEMENTS_PER_4844_BLOCK).map(|line| {
        let mut g1_bytes = [0_u8; 48];
        hex::decode_to_slice(line, &mut g1_bytes).expect("failed decoding G1 point from hex");
        let mut g1 = G1Compressed::empty();
        g1.as_mut().copy_from_slice(&g1_bytes);
        g1.into_affine().expect("invalid G1 point")
    });
    let mut lagrange_setup: Box<[_]> = lagrange_setup.collect();
    bit_reverse_slice_indices(&mut lagrange_setup);

    // Skip the 0th G2 point.
    assert!(
        lines.next().is_some(),
        "KZG trusted setup doesn't contain G2 points"
    );

    let line = lines
        .next()
        .expect("KZG trusted setup doesn't contain G2 point #1");
    let mut g2_bytes = [0_u8; 96];
    hex::decode_to_slice(line, &mut g2_bytes).expect("failed decoding G2 point from hex");
    let mut setup_g2_1 = G2Compressed::empty();

    setup_g2_1.as_mut().copy_from_slice(&g2_bytes);
    let setup_g2_1 = setup_g2_1
        .into_affine()
        .expect("invalid G2 point #1")
        .into_projective();

    KzgSettings {
        roots_of_unity_brp: roots_of_unity.try_into().unwrap(),
        setup_g2_1,
        lagrange_setup_brp: lagrange_setup.try_into().unwrap(),
    }
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kzg_roots_of_unity_are_correct() {
        let mut value = Fr::from_repr(FIRST_ROOT_OF_UNITY).unwrap();
        for _ in 0..ELEMENTS_PER_4844_BLOCK.ilog2() {
            assert_ne!(value, Fr::one());
            value.mul_assign(&value.clone());
        }
        assert_eq!(value, Fr::one());
    }
}
