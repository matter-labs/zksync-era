use zksync_basic_types::web3::signing::keccak256;
use zksync_basic_types::{MiniblockNumber, H256, U256};

pub fn miniblock_hash(miniblock_number: MiniblockNumber) -> H256 {
    H256(keccak256(&miniblock_number.0.to_be_bytes()))
}

pub const fn ceil_div(a: u64, b: u64) -> u64 {
    if a == 0 {
        a
    } else {
        (a - 1) / b + 1
    }
}

pub fn ceil_div_u256(a: U256, b: U256) -> U256 {
    (a + b - U256::from(1)) / b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ceil_div_u64_max() {
        assert_eq!(0, ceil_div(u64::MIN, u64::MAX));
        assert_eq!(1, ceil_div(u64::MAX, u64::MAX));
    }

    #[test]
    fn test_ceil_div_roundup_required() {
        assert_eq!(3, ceil_div(5, 2));
        assert_eq!(4, ceil_div(10, 3));
        assert_eq!(3, ceil_div(15, 7));
    }

    #[test]
    fn test_ceil_div_no_roundup_required() {
        assert_eq!(2, ceil_div(4, 2));
        assert_eq!(2, ceil_div(6, 3));
        assert_eq!(2, ceil_div(14, 7));
    }
}
