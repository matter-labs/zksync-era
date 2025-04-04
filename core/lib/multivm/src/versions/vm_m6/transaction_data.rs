use zk_evm_1_3_1::zkevm_opcode_defs::system_params::MAX_TX_ERGS_LIMIT;
use zksync_types::{
    address_to_h256,
    bytecode::BytecodeHash,
    ceil_div_u256,
    ethabi::{encode, Address, Token},
    fee::encoding_len,
    h256_to_u256,
    l1::is_l1_tx_type,
    l2::TransactionType,
    ExecuteTransactionCommon, Transaction, MAX_L2_TX_GAS_LIMIT, U256,
};

use super::vm_with_bootloader::{MAX_GAS_PER_PUBDATA_BYTE, MAX_TXS_IN_BLOCK};
use crate::{
    utils::bytecode::bytes_to_be_words,
    vm_m6::vm_with_bootloader::{
        BLOCK_OVERHEAD_GAS, BLOCK_OVERHEAD_PUBDATA, BOOTLOADER_TX_ENCODING_SPACE,
    },
};

pub(crate) const L1_TX_TYPE: u8 = 255;

// This structure represents the data that is used by
// the Bootloader to describe the transaction.
#[derive(Debug, Default, Clone)]
pub struct TransactionData {
    pub tx_type: u8,
    pub from: Address,
    pub to: Option<Address>,
    pub gas_limit: U256,
    pub pubdata_price_limit: U256,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub paymaster: Address,
    pub nonce: U256,
    pub value: U256,
    // The reserved fields that are unique for different types of transactions.
    // E.g. nonce is currently used in all transaction, but it should not be mandatory
    // in the long run.
    pub reserved: [U256; 4],
    pub data: Vec<u8>,
    pub signature: Vec<u8>,
    // The factory deps provided with the transaction.
    // Note that *only hashes* of these bytecodes are signed by the user
    // and they are used in the ABI encoding of the struct.
    // TODO: include this into the tx signature as part of SMA-1010
    pub factory_deps: Vec<Vec<u8>>,
    pub paymaster_input: Vec<u8>,
    pub reserved_dynamic: Vec<u8>,
}

impl From<Transaction> for TransactionData {
    fn from(execute_tx: Transaction) -> Self {
        match &execute_tx.common_data {
            ExecuteTransactionCommon::L2(common_data) => {
                let nonce = U256::from_big_endian(&common_data.nonce.to_be_bytes());

                let should_check_chain_id = if matches!(
                    common_data.transaction_type,
                    TransactionType::LegacyTransaction
                ) {
                    U256([1, 0, 0, 0])
                } else {
                    U256::zero()
                };

                // Ethereum transactions do not sign gas per pubdata limit, and so for them we need to use
                // some default value. We use the maximum possible value that is allowed by the bootloader
                // (i.e. we can not use u64::MAX, because the bootloader requires gas per pubdata for such
                // transactions to be higher than `MAX_GAS_PER_PUBDATA_BYTE`).
                let gas_per_pubdata_limit = if common_data.transaction_type.is_ethereum_type() {
                    MAX_GAS_PER_PUBDATA_BYTE.into()
                } else {
                    common_data.fee.gas_per_pubdata_limit
                };

                TransactionData {
                    tx_type: (common_data.transaction_type as u32) as u8,
                    from: execute_tx.initiator_account(),
                    to: execute_tx.execute.contract_address,
                    gas_limit: common_data.fee.gas_limit,
                    pubdata_price_limit: gas_per_pubdata_limit,
                    max_fee_per_gas: common_data.fee.max_fee_per_gas,
                    max_priority_fee_per_gas: common_data.fee.max_priority_fee_per_gas,
                    paymaster: common_data.paymaster_params.paymaster,
                    nonce,
                    value: execute_tx.execute.value,
                    reserved: [
                        should_check_chain_id,
                        U256::zero(),
                        U256::zero(),
                        U256::zero(),
                    ],
                    data: execute_tx.execute.calldata,
                    signature: common_data.signature.clone(),
                    factory_deps: execute_tx.execute.factory_deps,
                    paymaster_input: common_data.paymaster_params.paymaster_input.clone(),
                    reserved_dynamic: vec![],
                }
            }
            ExecuteTransactionCommon::L1(common_data) => {
                let refund_recipient = h256_to_u256(address_to_h256(&common_data.refund_recipient));
                TransactionData {
                    tx_type: L1_TX_TYPE,
                    from: common_data.sender,
                    to: execute_tx.execute.contract_address,
                    gas_limit: common_data.gas_limit,
                    pubdata_price_limit: common_data.gas_per_pubdata_limit,
                    // It doesn't matter what we put here, since
                    // the bootloader does not charge anything
                    max_fee_per_gas: common_data.max_fee_per_gas,
                    max_priority_fee_per_gas: U256::zero(),
                    paymaster: Address::default(),
                    nonce: U256::from(common_data.serial_id.0), // priority op ID
                    value: execute_tx.execute.value,
                    reserved: [
                        common_data.to_mint,
                        refund_recipient,
                        U256::zero(),
                        U256::zero(),
                    ],
                    data: execute_tx.execute.calldata,
                    // The signature isn't checked for L1 transactions so we don't care
                    signature: vec![],
                    factory_deps: execute_tx.execute.factory_deps,
                    paymaster_input: vec![],
                    reserved_dynamic: vec![],
                }
            }
            ExecuteTransactionCommon::ProtocolUpgrade(common_data) => {
                let refund_recipient = h256_to_u256(address_to_h256(&common_data.refund_recipient));
                TransactionData {
                    tx_type: common_data.tx_format() as u8,
                    from: common_data.sender,
                    to: execute_tx.execute.contract_address,
                    gas_limit: common_data.gas_limit,
                    pubdata_price_limit: common_data.gas_per_pubdata_limit,
                    // It doesn't matter what we put here, since
                    // the bootloader does not charge anything
                    max_fee_per_gas: common_data.max_fee_per_gas,
                    max_priority_fee_per_gas: U256::zero(),
                    paymaster: Address::default(),
                    nonce: U256::from(common_data.upgrade_id as u16),
                    value: execute_tx.execute.value,
                    reserved: [
                        common_data.to_mint,
                        refund_recipient,
                        U256::zero(),
                        U256::zero(),
                    ],
                    data: execute_tx.execute.calldata,
                    // The signature isn't checked for L1 transactions so we don't care
                    signature: vec![],
                    factory_deps: execute_tx.execute.factory_deps,
                    paymaster_input: vec![],
                    reserved_dynamic: vec![],
                }
            }
        }
    }
}

impl TransactionData {
    pub fn from(&self) -> Address {
        self.from
    }

    // This method is to be used only in tests, when we want to bypass the checks imposed
    // on the bytecode hash.
    pub(crate) fn abi_encode_with_custom_factory_deps(
        self,
        factory_deps_hashes: Vec<U256>,
    ) -> Vec<u8> {
        encode(&[Token::Tuple(vec![
            Token::Uint(U256::from_big_endian(&self.tx_type.to_be_bytes())),
            Token::Address(self.from),
            Token::Address(self.to.unwrap_or_default()),
            Token::Uint(self.gas_limit),
            Token::Uint(self.pubdata_price_limit),
            Token::Uint(self.max_fee_per_gas),
            Token::Uint(self.max_priority_fee_per_gas),
            Token::Address(self.paymaster),
            Token::Uint(self.nonce),
            Token::Uint(self.value),
            Token::FixedArray(self.reserved.iter().copied().map(Token::Uint).collect()),
            Token::Bytes(self.data),
            Token::Bytes(self.signature),
            Token::Array(factory_deps_hashes.into_iter().map(Token::Uint).collect()),
            Token::Bytes(self.paymaster_input),
            Token::Bytes(self.reserved_dynamic),
        ])])
    }

    pub(crate) fn abi_encode(self) -> Vec<u8> {
        let factory_deps_hashes = self
            .factory_deps
            .iter()
            .map(|dep| BytecodeHash::for_bytecode(dep).value_u256())
            .collect();
        self.abi_encode_with_custom_factory_deps(factory_deps_hashes)
    }

    pub fn into_tokens(self) -> Vec<U256> {
        bytes_to_be_words(&self.abi_encode())
    }

    pub(crate) fn effective_gas_price_per_pubdata(&self, block_gas_price_per_pubdata: u32) -> u32 {
        // It is enforced by the protocol that the L1 transactions always pay the exact amount of gas per pubdata
        // as was supplied in the transaction.
        if is_l1_tx_type(self.tx_type) {
            self.pubdata_price_limit.as_u32()
        } else {
            block_gas_price_per_pubdata
        }
    }

    pub fn overhead_gas(&self, block_gas_price_per_pubdata: u32) -> u32 {
        let total_gas_limit = self.gas_limit.as_u32();
        let gas_price_per_pubdata =
            self.effective_gas_price_per_pubdata(block_gas_price_per_pubdata);

        let encoded_len = encoding_len(
            self.data.len() as u64,
            self.signature.len() as u64,
            self.factory_deps.len() as u64,
            self.paymaster_input.len() as u64,
            self.reserved_dynamic.len() as u64,
        );

        let coefficients = OverheadCoefficients::from_tx_type(self.tx_type);
        get_amortized_overhead(
            total_gas_limit,
            gas_price_per_pubdata,
            encoded_len,
            coefficients,
        )
    }

    pub fn trusted_gas_limit(&self, _block_gas_price_per_pubdata: u32) -> u32 {
        // TODO (EVM-66): correctly calculate the trusted gas limit for a transaction
        self.gas_limit.as_u32()
    }
}

pub(crate) fn derive_overhead(
    gas_limit: u64,
    gas_price_per_pubdata: u32,
    encoded_len: usize,
    coefficients: OverheadCoefficients,
) -> u32 {
    // Even if the gas limit is greater than the `MAX_TX_ERGS_LIMIT`, we assume that everything beyond `MAX_TX_ERGS_LIMIT`
    // will be spent entirely on publishing bytecodes and so we derive the overhead solely based on the capped value
    let gas_limit = std::cmp::min(MAX_TX_ERGS_LIMIT as u64, gas_limit);

    // Using large U256 type to avoid overflow
    let max_block_overhead = U256::from(block_overhead_gas(gas_price_per_pubdata));
    let gas_limit = U256::from(gas_limit);
    let encoded_len = U256::from(encoded_len);

    // The `MAX_TX_ERGS_LIMIT` is formed in a way that may fulfills a single-instance circuits
    // if used in full. That is, within `MAX_TX_ERGS_LIMIT` it is possible to fully saturate all the single-instance
    // circuits.
    let overhead_for_single_instance_circuits =
        ceil_div_u256(gas_limit * max_block_overhead, MAX_TX_ERGS_LIMIT.into());

    // The overhead for occupying the bootloader memory
    let overhead_for_length = ceil_div_u256(
        encoded_len * max_block_overhead,
        BOOTLOADER_TX_ENCODING_SPACE.into(),
    );

    // The overhead for occupying a single tx slot
    let tx_slot_overhead = ceil_div_u256(max_block_overhead, MAX_TXS_IN_BLOCK.into());

    // We use `ceil` here for formal reasons to allow easier approach for calculating the overhead in O(1)
    // `let max_pubdata_in_tx = ceil_div_u256(gas_limit, gas_price_per_pubdata);`

    // The maximal potential overhead from pubdata
    // TODO (EVM-67): possibly use overhead for pubdata
    // ```
    // let pubdata_overhead = ceil_div_u256(
    //     max_pubdata_in_tx * max_block_overhead,
    //     MAX_PUBDATA_PER_BLOCK.into(),
    // );
    // ```
    vec![
        (coefficients.ergs_limit_overhead_coeficient
            * overhead_for_single_instance_circuits.as_u32() as f64)
            .floor() as u32,
        (coefficients.bootloader_memory_overhead_coeficient * overhead_for_length.as_u32() as f64)
            .floor() as u32,
        (coefficients.slot_overhead_coeficient * tx_slot_overhead.as_u32() as f64) as u32,
    ]
    .into_iter()
    .max()
    .unwrap()
}

/// Contains the coefficients with which the overhead for transactions will be calculated.
///
/// All of the coefficients should be <= 1. There are here to provide a certain "discount" for normal transactions
/// at the risk of malicious transactions that may close the block prematurely.
/// IMPORTANT: to perform correct computations, `MAX_TX_ERGS_LIMIT / coefficients.ergs_limit_overhead_coefficient` MUST
/// result in an integer number
#[derive(Debug, Clone, Copy)]
pub struct OverheadCoefficients {
    slot_overhead_coeficient: f64,
    bootloader_memory_overhead_coeficient: f64,
    ergs_limit_overhead_coeficient: f64,
}

impl OverheadCoefficients {
    // This method ensures that the parameters keep the required invariants
    fn new_checked(
        slot_overhead_coeficient: f64,
        bootloader_memory_overhead_coeficient: f64,
        ergs_limit_overhead_coeficient: f64,
    ) -> Self {
        assert!(
            (MAX_TX_ERGS_LIMIT as f64 / ergs_limit_overhead_coeficient).round()
                == MAX_TX_ERGS_LIMIT as f64 / ergs_limit_overhead_coeficient,
            "MAX_TX_ERGS_LIMIT / ergs_limit_overhead_coeficient must be an integer"
        );

        Self {
            slot_overhead_coeficient,
            bootloader_memory_overhead_coeficient,
            ergs_limit_overhead_coeficient,
        }
    }

    // L1->L2 do not receive any discounts
    fn new_l1() -> Self {
        OverheadCoefficients::new_checked(1.0, 1.0, 1.0)
    }

    fn new_l2() -> Self {
        OverheadCoefficients::new_checked(
            1.0, 1.0,
            // For L2 transactions we allow a certain default discount with regard to the number of ergs.
            // Multi-instance circuits can in theory be spawned infinite times, while projected future limitations
            // on gas per pubdata allow for roughly 800k gas per L1 batch, so the rough trust "discount" on the proof's part
            // to be paid by the users is 0.1.
            0.1,
        )
    }

    pub fn from_tx_type(tx_type: u8) -> Self {
        if tx_type == L1_TX_TYPE {
            Self::new_l1()
        } else {
            Self::new_l2()
        }
    }
}

/// This method returns the overhead for processing the block  
pub fn get_amortized_overhead(
    total_gas_limit: u32,
    gas_per_pubdata_byte_limit: u32,
    encoded_len: usize,
    coefficients: OverheadCoefficients,
) -> u32 {
    // Using large U256 type to prevent overflows.
    let overhead_for_block_gas = U256::from(block_overhead_gas(gas_per_pubdata_byte_limit));
    let total_gas_limit = U256::from(total_gas_limit);
    let encoded_len = U256::from(encoded_len);

    // Derivation of overhead consists of 4 parts:
    // 1. The overhead for taking up a transaction's slot. `(O1): O1 = 1 / MAX_TXS_IN_BLOCK`
    // 2. The overhead for taking up the bootloader's memory `(O2): O2 = encoded_len / BOOTLOADER_TX_ENCODING_SPACE`
    // 3. The overhead for possible usage of pubdata. `(O3): O3 = max_pubdata_in_tx / MAX_PUBDATA_PER_BLOCK`
    // 4. The overhead for possible usage of all the single-instance circuits. `(O4): O4 = gas_limit / MAX_TX_ERGS_LIMIT`
    //
    // The maximum of these is taken to derive the part of the block's overhead to be paid by the users:
    //
    // `max_overhead = max(O1, O2, O3, O4)`
    // `overhead_gas = ceil(max_overhead * overhead_for_block_gas)`. Thus, `overhead_gas` is a function of
    // `tx_gas_limit`, `gas_per_pubdata_byte_limit` and `encoded_len`.
    //
    // While it is possible to derive the overhead with binary search in `O(log n)`, it is too expensive to be done
    // on L1, so here is a reference implementation of finding the overhead for transaction in O(1):
    //
    // Given `total_gas_limit = tx_gas_limit + overhead_gas`, we need to find `overhead_gas` and `tx_gas_limit`, such that:
    // 1. `overhead_gas` is maximal possible (the operator is paid fairly)
    // 2. `overhead_gas(tx_gas_limit, gas_per_pubdata_byte_limit, encoded_len) >= overhead_gas` (the user does not overpay)
    // The third part boils to the following 4 inequalities (at least one of these must hold):
    // `ceil(O1 * overhead_for_block_gas) >= overhead_gas`
    // `ceil(O2 * overhead_for_block_gas) >= overhead_gas`
    // `ceil(O3 * overhead_for_block_gas) >= overhead_gas`
    // `ceil(O4 * overhead_for_block_gas) >= overhead_gas`
    //
    // Now, we need to solve each of these separately:

    // 1. The overhead for occupying a single tx slot is a constant:
    let tx_slot_overhead = {
        let tx_slot_overhead =
            ceil_div_u256(overhead_for_block_gas, MAX_TXS_IN_BLOCK.into()).as_u32();
        (coefficients.slot_overhead_coeficient * tx_slot_overhead as f64).floor() as u32
    };

    // 2. The overhead for occupying the bootloader memory can be derived from `encoded_len`
    let overhead_for_length = {
        let overhead_for_length = ceil_div_u256(
            encoded_len * overhead_for_block_gas,
            BOOTLOADER_TX_ENCODING_SPACE.into(),
        )
        .as_u32();

        (coefficients.bootloader_memory_overhead_coeficient * overhead_for_length as f64).floor()
            as u32
    };

    // TODO (EVM-67): possibly include the overhead for pubdata. The formula below has not been properly maintained,
    // since the pubdata is not published. If decided to use the pubdata overhead, it needs to be updated.
    // ```
    // 3. ceil(O3 * overhead_for_block_gas) >= overhead_gas
    // O3 = max_pubdata_in_tx / MAX_PUBDATA_PER_BLOCK = ceil(gas_limit / gas_per_pubdata_byte_limit) / MAX_PUBDATA_PER_BLOCK
    // >= (gas_limit / (gas_per_pubdata_byte_limit * MAX_PUBDATA_PER_BLOCK).
    // ```
    // Throwing off the `ceil`, while may provide marginally lower
    // overhead to the operator, provides substantially easier formula to work with.
    //
    // For better clarity, let's denote `gas_limit = GL, MAX_PUBDATA_PER_BLOCK = MP, gas_per_pubdata_byte_limit = EP, overhead_for_block_gas = OB, total_gas_limit = TL, overhead_gas = OE`
    // `ceil(OB * (TL - OE) / (EP * MP)) >= OE`
    // ```
    // OB * (TL - OE) / (MP * EP) > OE - 1
    // OB * (TL - OE) > (OE - 1) * EP * MP
    // OB * TL + EP * MP > OE * EP * MP + OE * OB
    // (OB * TL + EP * MP) / (EP * MP + OB) > OE
    // OE = floor((OB * TL + EP * MP) / (EP * MP + OB)) with possible -1 if the division is without remainder
    // let overhead_for_pubdata = {
    //     let numerator: U256 = overhead_for_block_gas * total_gas_limit
    //         + gas_per_pubdata_byte_limit * U256::from(MAX_PUBDATA_PER_BLOCK);
    //     let denominator =
    //         gas_per_pubdata_byte_limit * U256::from(MAX_PUBDATA_PER_BLOCK) + overhead_for_block_gas;
    //
    //     // Corner case: if `total_gas_limit` = `gas_per_pubdata_byte_limit` = 0
    //     // then the numerator will be 0 and subtracting 1 will cause a panic, so we just return a zero.
    //     if numerator.is_zero() {
    //         0.into()
    //     } else {
    //         (numerator - 1) / denominator
    //     }
    // };
    //
    // 4. K * ceil(O4 * overhead_for_block_gas) >= overhead_gas, where K is the discount
    // O4 = gas_limit / MAX_TX_ERGS_LIMIT. Using the notation from the previous equation:
    // ceil(OB * GL / MAX_TX_ERGS_LIMIT) >= (OE / K)
    // ceil(OB * (TL - OE) / MAX_TX_ERGS_LIMIT) >= (OE/K)
    // OB * (TL - OE) / MAX_TX_ERGS_LIMIT > (OE/K) - 1
    // OB * (TL - OE) > (OE/K) * MAX_TX_ERGS_LIMIT - MAX_TX_ERGS_LIMIT
    // OB * TL + MAX_TX_ERGS_LIMIT > OE * ( MAX_TX_ERGS_LIMIT/K + OB)
    // OE = floor(OB * TL + MAX_TX_ERGS_LIMIT / (MAX_TX_ERGS_LIMIT/K + OB)), with possible -1 if the division is without remainder
    // ```
    let overhead_for_gas = {
        let numerator = overhead_for_block_gas * total_gas_limit + U256::from(MAX_TX_ERGS_LIMIT);
        let denominator: U256 = U256::from(
            (MAX_TX_ERGS_LIMIT as f64 / coefficients.ergs_limit_overhead_coeficient) as u64,
        ) + overhead_for_block_gas;

        let overhead_for_gas = (numerator - 1) / denominator;

        overhead_for_gas.as_u32()
    };

    let overhead = vec![tx_slot_overhead, overhead_for_length, overhead_for_gas]
        .into_iter()
        .max()
        // For the sake of consistency making sure that `total_gas_limit >= max_overhead`
        .map(|max_overhead| std::cmp::min(max_overhead, total_gas_limit.as_u32()))
        .unwrap();

    let limit_after_deducting_overhead = total_gas_limit - overhead;

    // During double checking of the overhead, the bootloader will assume that the
    // body of the transaction does not have any more than `MAX_L2_TX_GAS_LIMIT` ergs available to it.
    if limit_after_deducting_overhead.as_u64() > MAX_L2_TX_GAS_LIMIT {
        // We derive the same overhead that would exist for the `MAX_L2_TX_GAS_LIMIT` ergs
        derive_overhead(
            MAX_L2_TX_GAS_LIMIT,
            gas_per_pubdata_byte_limit,
            encoded_len.as_usize(),
            coefficients,
        )
    } else {
        overhead
    }
}

pub(crate) fn block_overhead_gas(gas_per_pubdata_byte: u32) -> u32 {
    BLOCK_OVERHEAD_GAS + BLOCK_OVERHEAD_PUBDATA * gas_per_pubdata_byte
}

#[cfg(test)]
mod tests {

    use zksync_types::fee::encoding_len;

    use super::*;

    // This method returns the maximum block overhead that can be charged from the user based on the binary search approach
    pub fn get_maximal_allowed_overhead_bin_search(
        total_gas_limit: u32,
        gas_per_pubdata_byte_limit: u32,
        encoded_len: usize,
        coefficients: OverheadCoefficients,
    ) -> u32 {
        let mut left_bound = if MAX_TX_ERGS_LIMIT < total_gas_limit {
            total_gas_limit - MAX_TX_ERGS_LIMIT
        } else {
            0u32
        };
        // Safe cast: the gas_limit for a transaction can not be larger than `2^32`
        let mut right_bound = total_gas_limit;

        // The closure returns whether a certain overhead would be accepted by the bootloader.
        // It is accepted if the derived overhead (i.e. the actual overhead that the user has to pay)
        // is >= than the overhead proposed by the operator.
        let is_overhead_accepted = |suggested_overhead: u32| {
            let derived_overhead = derive_overhead(
                (total_gas_limit - suggested_overhead) as u64,
                gas_per_pubdata_byte_limit,
                encoded_len,
                coefficients,
            );

            derived_overhead >= suggested_overhead
        };

        // In order to find the maximal allowed overhead we are doing binary search
        while left_bound + 1 < right_bound {
            let mid = (left_bound + right_bound) / 2;

            if is_overhead_accepted(mid) {
                left_bound = mid;
            } else {
                right_bound = mid;
            }
        }

        if is_overhead_accepted(right_bound) {
            right_bound
        } else {
            left_bound
        }
    }

    #[test]
    fn test_correctness_for_efficient_overhead() {
        let test_params = |total_gas_limit: u32,
                           gas_per_pubdata: u32,
                           encoded_len: usize,
                           coefficients: OverheadCoefficients| {
            let result_by_efficient_search =
                get_amortized_overhead(total_gas_limit, gas_per_pubdata, encoded_len, coefficients);

            let result_by_binary_search = get_maximal_allowed_overhead_bin_search(
                total_gas_limit,
                gas_per_pubdata,
                encoded_len,
                coefficients,
            );

            assert_eq!(result_by_efficient_search, result_by_binary_search);
        };

        // Some arbitrary test
        test_params(60_000_000, 800, 2900, OverheadCoefficients::new_l2());

        // Very small parameters
        test_params(0, 1, 12, OverheadCoefficients::new_l2());

        // Relatively big parameters
        let max_tx_overhead = derive_overhead(
            MAX_TX_ERGS_LIMIT as u64,
            5000,
            10000,
            OverheadCoefficients::new_l2(),
        );
        test_params(
            MAX_TX_ERGS_LIMIT + max_tx_overhead,
            5000,
            10000,
            OverheadCoefficients::new_l2(),
        );

        test_params(115432560, 800, 2900, OverheadCoefficients::new_l1());
    }

    #[test]
    fn test_consistency_with_encoding_length() {
        let transaction = TransactionData {
            tx_type: 113,
            from: Address::random(),
            to: Some(Address::random()),
            gas_limit: U256::from(1u32),
            pubdata_price_limit: U256::from(1u32),
            max_fee_per_gas: U256::from(1u32),
            max_priority_fee_per_gas: U256::from(1u32),
            paymaster: Address::random(),
            nonce: U256::zero(),
            value: U256::zero(),
            // The reserved fields that are unique for different types of transactions.
            // E.g. nonce is currently used in all transaction, but it should not be mandatory
            // in the long run.
            reserved: [U256::zero(); 4],
            data: vec![0u8; 65],
            signature: vec![0u8; 75],
            // The factory deps provided with the transaction.
            // Note that *only hashes* of these bytecodes are signed by the user
            // and they are used in the ABI encoding of the struct.
            // TODO: include this into the tx signature as part of SMA-1010
            factory_deps: vec![vec![0u8; 32], vec![1u8; 32]],
            paymaster_input: vec![0u8; 85],
            reserved_dynamic: vec![0u8; 32],
        };

        let assumed_encoded_len = encoding_len(65, 75, 2, 85, 32);

        let true_encoding_len = transaction.into_tokens().len();

        assert_eq!(assumed_encoded_len, true_encoding_len);
    }
}
