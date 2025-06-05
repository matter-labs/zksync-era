use ruint::aliases::B160;
use zk_ee::{common_structs::derive_flat_storage_key, utils::Bytes32};
use zk_os_forward_system::run::{
    test_impl::{InMemoryPreimageSource, InMemoryTree},
    Log,
};
use zksync_types::{
    address_to_h256,
    bytecode::BytecodeHash,
    ethabi::{encode, Address, Token},
    h256_to_u256,
    l2::TransactionType,
    ExecuteTransactionCommon, L1BatchNumber, Transaction, H256, U256,
};
use zksync_vm_interface::VmEvent;

pub(crate) const MAX_GAS_PER_PUBDATA_BYTE: u64 = 50_000;

pub fn tx_abi_encode(tx: Transaction) -> Vec<u8> {
    let tx_data: TransactionData = tx.into();
    let encoded = tx_data.abi_encode();
    encoded
}

#[derive(Debug, Default, Clone)]
pub(crate) struct TransactionData {
    pub(crate) tx_type: u8,
    pub(crate) from: Address,
    pub(crate) to: Option<Address>,
    pub(crate) gas_limit: U256,
    pub(crate) pubdata_price_limit: U256,
    pub(crate) max_fee_per_gas: U256,
    pub(crate) max_priority_fee_per_gas: U256,
    pub(crate) paymaster: Address,
    pub(crate) nonce: U256,
    pub(crate) value: U256,
    // The reserved fields that are unique for different types of transactions.
    // E.g. nonce is currently used in all transaction, but it should not be mandatory
    // in the long run.
    pub(crate) reserved: [U256; 4],
    pub(crate) data: Vec<u8>,
    pub(crate) signature: Vec<u8>,
    // The factory deps provided with the transaction.
    // Note that *only hashes* of these bytecodes are signed by the user
    // and they are used in the ABI encoding of the struct.
    // TODO: include this into the tx signature as part of SMA-1010
    pub(crate) factory_deps: Vec<H256>,
    pub(crate) paymaster_input: Vec<u8>,
    pub(crate) reserved_dynamic: Vec<u8>,
    pub(crate) raw_bytes: Option<Vec<u8>>,
}

impl TransactionData {
    pub fn abi_encode(self) -> Vec<u8> {
        let mut res = encode(&[Token::Tuple(vec![
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
            Token::Array(
                self.factory_deps
                    .into_iter()
                    .map(|dep| Token::FixedBytes(dep.as_bytes().to_vec()))
                    .collect(),
            ),
            Token::Bytes(self.paymaster_input),
            Token::Bytes(self.reserved_dynamic),
        ])]);

        res.drain(0..32);
        res
    }
}

impl From<Transaction> for TransactionData {
    fn from(execute_tx: Transaction) -> Self {
        match execute_tx.common_data {
            ExecuteTransactionCommon::L2(common_data) => {
                let nonce = U256::from_big_endian(&common_data.nonce.to_be_bytes());

                let should_check_chain_id = if matches!(
                    common_data.transaction_type,
                    TransactionType::LegacyTransaction
                ) && common_data.extract_chain_id().is_some()
                {
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

                let is_deployment_transaction = match execute_tx.execute.contract_address {
                    None =>
                    // that means it's a deploy transaction
                    {
                        U256([1, 0, 0, 0])
                    }
                    // all other transactions
                    Some(_) => U256::zero(),
                };

                TransactionData {
                    tx_type: (common_data.transaction_type as u32) as u8,
                    from: common_data.initiator_address,
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
                        is_deployment_transaction,
                        U256::zero(),
                        U256::zero(),
                    ],
                    data: execute_tx.execute.calldata,
                    signature: common_data.signature,
                    // todo: currently zksync_os cannot process factory_deps due to a bug
                    factory_deps: execute_tx
                        .execute
                        .factory_deps
                        .iter()
                        .map(|b| BytecodeHash::for_bytecode(b).value())
                        .collect(),
                    paymaster_input: common_data.paymaster_params.paymaster_input,
                    reserved_dynamic: vec![],
                    raw_bytes: execute_tx.raw_bytes.map(|a| a.0),
                }
            }
            ExecuteTransactionCommon::L1(common_data) => {
                // TODO: cleanup - double check gas fields, and sender, use constant for tx type
                TransactionData {
                    tx_type: 255,
                    from: common_data.sender,
                    to: execute_tx.execute.contract_address,
                    gas_limit: common_data.gas_limit,
                    pubdata_price_limit: common_data.gas_per_pubdata_limit,
                    max_fee_per_gas: common_data.max_fee_per_gas,
                    max_priority_fee_per_gas: U256::zero(),
                    paymaster: Address::zero(),
                    nonce: U256::from(common_data.serial_id.0),
                    value: execute_tx.execute.value,
                    reserved: [
                        common_data.to_mint,
                        U256::from_big_endian(common_data.refund_recipient.as_bytes()),
                        U256::zero(),
                        U256::zero(),
                    ],
                    data: execute_tx.execute.calldata,
                    signature: vec![],
                    factory_deps: execute_tx
                        .execute
                        .factory_deps
                        .iter()
                        .map(|b| BytecodeHash::for_bytecode(b).value())
                        .collect(),
                    paymaster_input: vec![],
                    reserved_dynamic: vec![],
                    raw_bytes: execute_tx.raw_bytes.map(|a| a.0),
                }
            }
            ExecuteTransactionCommon::ProtocolUpgrade(_) => {
                unreachable!()
            }
        }
    }
}

pub fn h256_to_bytes32(input: H256) -> Bytes32 {
    let mut new = Bytes32::zero();
    new.as_u8_array_mut().copy_from_slice(input.as_bytes());
    new
}

pub fn bytes32_to_h256(input: Bytes32) -> H256 {
    H256(input.as_u8_array())
}

pub fn b160_to_address(input: B160) -> Address {
    Address::from_slice(&input.to_be_bytes::<20>())
}

pub fn zkos_log_to_vm_event(log: Log, location: (L1BatchNumber, u32)) -> VmEvent {
    VmEvent {
        location,
        address: b160_to_address(log.address),
        indexed_topics: log.topics.into_iter().map(bytes32_to_h256).collect(),
        value: log.data,
    }
}
