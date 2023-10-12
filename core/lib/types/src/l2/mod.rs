use std::convert::TryFrom;

use num_enum::TryFromPrimitive;
use rlp::{Rlp, RlpStream};

use self::error::SignError;
use crate::transaction_request::PaymasterParams;
use crate::LEGACY_TX_TYPE;

use crate::{
    api, tx::primitives::PackedEthSignature, tx::Execute, web3::types::U64, Address, Bytes,
    EIP712TypedStructure, Eip712Domain, ExecuteTransactionCommon, InputData, L2ChainId, Nonce,
    StructBuilder, Transaction, EIP_1559_TX_TYPE, EIP_2930_TX_TYPE, EIP_712_TX_TYPE, H256,
    PRIORITY_OPERATION_L2_TX_TYPE, PROTOCOL_UPGRADE_TX_TYPE, U256,
};

use serde::{Deserialize, Serialize};

pub mod error;

use crate::api::TransactionRequest;
use crate::fee::{encoding_len, Fee};
use crate::helpers::unix_timestamp_ms;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u32)]
pub enum TransactionType {
    // Native ECDSA Transaction
    LegacyTransaction = 0,

    EIP2930Transaction = 1,
    EIP1559Transaction = 2,
    // Eip 712 transaction with additional fields specified for zksync
    EIP712Transaction = EIP_712_TX_TYPE as u32,
    PriorityOpTransaction = PRIORITY_OPERATION_L2_TX_TYPE as u32,
    ProtocolUpgradeTransaction = PROTOCOL_UPGRADE_TX_TYPE as u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L2TxCommonData {
    pub nonce: Nonce,
    pub fee: Fee,
    pub initiator_address: Address,
    pub signature: Vec<u8>,
    pub transaction_type: TransactionType,
    /// This input consists of raw transaction bytes when we receive it from API.    
    /// But we still use this structure for zksync-rs and tests, and we don't have raw tx before
    /// creating the structure. We setup this field manually later for consistency.    
    /// We need some research on how to change it
    pub input: Option<InputData>,

    pub paymaster_params: PaymasterParams,
}

impl L2TxCommonData {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nonce: Nonce,
        fee: Fee,
        initiator_address: Address,
        signature: Vec<u8>,
        transaction_type: TransactionType,
        input: Vec<u8>,
        hash: H256,
        paymaster_params: PaymasterParams,
    ) -> Self {
        let input = Some(InputData { hash, data: input });
        Self {
            nonce,
            fee,
            initiator_address,
            signature,
            transaction_type,
            input,
            paymaster_params,
        }
    }

    pub fn input_data(&self) -> Option<&[u8]> {
        self.input.as_ref().map(|input| &*input.data)
    }

    /// Returns zero hash if the transaction doesn't contain input data.
    pub fn hash(&self) -> H256 {
        self.input
            .as_ref()
            .expect("Transaction must have input data")
            .hash
    }

    pub fn set_input(&mut self, input: Vec<u8>, hash: H256) {
        self.input = Some(InputData { hash, data: input })
    }

    pub fn extract_chain_id(&self) -> Option<u64> {
        let bytes = self.input_data()?;
        let chain_id = match bytes.first() {
            Some(x) if *x >= 0x80 => {
                let rlp = Rlp::new(bytes);
                let v = rlp.val_at(6).ok()?;
                PackedEthSignature::unpack_v(v).ok()?.1?
            }
            Some(x) if *x == EIP_1559_TX_TYPE => {
                let rlp = Rlp::new(&bytes[1..]);
                rlp.val_at(0).ok()?
            }
            Some(x) if *x == EIP_712_TX_TYPE => {
                let rlp = Rlp::new(&bytes[1..]);
                rlp.val_at(10).ok()?
            }
            _ => return None,
        };
        Some(chain_id)
    }
}

impl Default for L2TxCommonData {
    fn default() -> Self {
        Self {
            nonce: Nonce(0),
            fee: Default::default(),
            initiator_address: Address::zero(),
            signature: Default::default(),
            transaction_type: TransactionType::EIP712Transaction,
            input: Default::default(),
            paymaster_params: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct L2Tx {
    pub execute: Execute,
    pub common_data: L2TxCommonData,
    pub received_timestamp_ms: u64,
    pub raw_bytes: Option<Bytes>,
}

impl L2Tx {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        contract_address: Address,
        calldata: Vec<u8>,
        nonce: Nonce,
        fee: Fee,
        initiator_address: Address,
        value: U256,
        factory_deps: Option<Vec<Vec<u8>>>,
        paymaster_params: PaymasterParams,
    ) -> Self {
        Self {
            execute: Execute {
                contract_address,
                calldata,
                value,
                factory_deps,
            },
            common_data: L2TxCommonData {
                nonce,
                fee,
                initiator_address,
                signature: Default::default(),
                transaction_type: TransactionType::EIP712Transaction,
                input: None,
                paymaster_params,
            },
            received_timestamp_ms: unix_timestamp_ms(),
            raw_bytes: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_signed(
        contract_address: Address,
        calldata: Vec<u8>,
        nonce: Nonce,
        fee: Fee,
        value: U256,
        chain_id: L2ChainId,
        private_key: &H256,
        factory_deps: Option<Vec<Vec<u8>>>,
        paymaster_params: PaymasterParams,
    ) -> Result<Self, SignError> {
        let initiator_address = PackedEthSignature::address_from_private_key(private_key).unwrap();
        let mut res = Self::new(
            contract_address,
            calldata,
            nonce,
            fee,
            initiator_address,
            value,
            factory_deps,
            paymaster_params,
        );

        let data = res.get_signed_bytes(chain_id);
        res.set_signature(PackedEthSignature::sign_raw(private_key, &data)?);
        Ok(res)
    }

    /// Returns the hash of the transaction.
    pub fn hash(&self) -> H256 {
        self.common_data.hash()
    }

    /// Returns the account affected by the transaction.
    pub fn initiator_account(&self) -> Address {
        self.common_data.initiator_address
    }

    /// Returns recipient account of the transaction.
    pub fn recipient_account(&self) -> Address {
        self.execute.contract_address
    }

    /// Returns the account nonce associated with transaction.
    pub fn nonce(&self) -> Nonce {
        self.common_data.nonce
    }

    pub fn set_input(&mut self, data: Vec<u8>, hash: H256) {
        self.common_data.set_input(data, hash)
    }

    pub fn get_rlp_bytes(&self, chain_id: L2ChainId) -> Bytes {
        let mut rlp_stream = RlpStream::new();
        let tx: TransactionRequest = self.clone().into();
        tx.rlp(&mut rlp_stream, chain_id.as_u64(), None);
        Bytes(rlp_stream.as_raw().to_vec())
    }

    pub fn get_signed_bytes(&self, chain_id: L2ChainId) -> H256 {
        let tx: TransactionRequest = self.clone().into();
        if tx.is_eip712_tx() {
            PackedEthSignature::typed_data_to_signed_bytes(&Eip712Domain::new(chain_id), &tx)
        } else {
            let mut data = self.get_rlp_bytes(chain_id).0;
            if let Some(tx_type) = tx.transaction_type {
                data.insert(0, tx_type.as_u32() as u8);
            }
            PackedEthSignature::message_to_signed_bytes(&data)
        }
    }

    pub fn set_signature(&mut self, signature: PackedEthSignature) {
        self.set_raw_signature(signature.serialize_packed().to_vec());
    }

    pub fn set_raw_signature(&mut self, signature: Vec<u8>) {
        self.common_data.signature = signature;
    }

    pub fn set_raw_bytes(&mut self, bytes: Bytes) {
        self.raw_bytes = Some(bytes);
    }

    pub fn abi_encoding_len(&self) -> usize {
        let data_len = self.execute.calldata.len();
        let signature_len = self.common_data.signature.len();
        let factory_deps_len = self.execute.factory_deps_length();
        let paymaster_input_len = self.common_data.paymaster_params.paymaster_input.len();

        encoding_len(
            data_len as u64,
            signature_len as u64,
            factory_deps_len as u64,
            paymaster_input_len as u64,
            0,
        )
    }

    pub fn payer(&self) -> Address {
        if self.common_data.paymaster_params.paymaster != Address::zero() {
            self.common_data.paymaster_params.paymaster
        } else {
            self.initiator_account()
        }
    }

    pub fn factory_deps_len(&self) -> u32 {
        self.execute
            .factory_deps
            .as_ref()
            .map(|deps| deps.iter().fold(0u32, |len, item| len + item.len() as u32))
            .unwrap_or_default()
    }
}

fn signature_to_vrs(signature: &[u8], tx_type: u32) -> (Option<U64>, Option<U256>, Option<U256>) {
    let signature = if tx_type == LEGACY_TX_TYPE as u32 {
        // Note that we use `deserialize_packed_no_v_check` here, because we want to preserve the original `v` value.
        // This is needed due to inconsistent behaviour on Ethereum where the `v` value is >= 27 for legacy transactions
        // and is either 0 or 1 for other ones.
        PackedEthSignature::deserialize_packed_no_v_check(signature)
    } else {
        PackedEthSignature::deserialize_packed(signature)
    };

    if let Ok(sig) = signature {
        (
            Some(U64::from(sig.v())),
            Some(U256::from(sig.r())),
            Some(U256::from(sig.s())),
        )
    } else {
        (None, None, None)
    }
}

impl From<L2Tx> for TransactionRequest {
    fn from(tx: L2Tx) -> Self {
        let tx_type = tx.common_data.transaction_type as u32;
        let (v, r, s) = signature_to_vrs(&tx.common_data.signature, tx_type);

        let mut base_tx_req = TransactionRequest {
            nonce: U256::from(tx.common_data.nonce.0),
            from: Some(tx.common_data.initiator_address),
            to: Some(tx.recipient_account()),
            value: tx.execute.value,
            gas_price: tx.common_data.fee.max_fee_per_gas,
            max_priority_fee_per_gas: None,
            gas: tx.common_data.fee.gas_limit,
            input: Bytes(tx.execute.calldata),
            v,
            r,
            s,
            raw: tx.raw_bytes,
            transaction_type: None,
            access_list: None,
            eip712_meta: None,
            chain_id: tx.common_data.extract_chain_id(),
        };
        match tx_type as u8 {
            LEGACY_TX_TYPE => {}

            EIP_712_TX_TYPE => {
                base_tx_req.transaction_type = Some(U64::from(tx_type));
                base_tx_req.max_priority_fee_per_gas =
                    Some(tx.common_data.fee.max_priority_fee_per_gas);
                base_tx_req.eip712_meta = Some(api::Eip712Meta {
                    gas_per_pubdata: tx.common_data.fee.gas_per_pubdata_limit,
                    factory_deps: tx.execute.factory_deps,
                    custom_signature: Some(tx.common_data.signature),
                    paymaster_params: Some(tx.common_data.paymaster_params),
                });
            }
            EIP_1559_TX_TYPE | EIP_2930_TX_TYPE => {
                base_tx_req.max_priority_fee_per_gas =
                    Some(tx.common_data.fee.max_priority_fee_per_gas);
                base_tx_req.transaction_type = Some(U64::from(tx_type));
            }
            _ => panic!("Invalid transaction type: {}", tx_type),
        }
        base_tx_req
    }
}

impl From<L2Tx> for Transaction {
    fn from(tx: L2Tx) -> Self {
        let L2Tx {
            execute,
            common_data,
            received_timestamp_ms,
            raw_bytes,
        } = tx;
        Self {
            common_data: ExecuteTransactionCommon::L2(common_data),
            execute,
            received_timestamp_ms,
            raw_bytes,
        }
    }
}

impl From<L2Tx> for api::Transaction {
    fn from(tx: L2Tx) -> Self {
        let tx_type = tx.common_data.transaction_type as u32;
        let (v, r, s) =
            if let Ok(sig) = PackedEthSignature::deserialize_packed(&tx.common_data.signature) {
                (
                    Some(U64::from(sig.v())),
                    Some(U256::from(sig.r())),
                    Some(U256::from(sig.s())),
                )
            } else {
                (None, None, None)
            };

        Self {
            hash: tx.hash(),
            chain_id: U256::from(tx.common_data.extract_chain_id().unwrap_or_default()),
            nonce: U256::from(tx.common_data.nonce.0),
            from: Some(tx.common_data.initiator_address),
            to: Some(tx.recipient_account()),
            value: tx.execute.value,
            gas_price: Some(tx.common_data.fee.max_fee_per_gas),
            max_priority_fee_per_gas: Some(tx.common_data.fee.max_priority_fee_per_gas),
            max_fee_per_gas: Some(tx.common_data.fee.max_fee_per_gas),
            gas: tx.common_data.fee.gas_limit,
            input: Bytes(tx.execute.calldata),
            v,
            r,
            s,
            transaction_type: if tx_type == 0 {
                None
            } else {
                Some(U64::from(tx_type))
            },
            ..Default::default()
        }
    }
}

impl TryFrom<Transaction> for L2Tx {
    type Error = &'static str;

    fn try_from(value: Transaction) -> Result<Self, Self::Error> {
        let Transaction {
            common_data,
            execute,
            received_timestamp_ms,
            raw_bytes,
        } = value;
        match common_data {
            ExecuteTransactionCommon::L1(_) => Err("Cannot convert L1Tx to L2Tx"),
            ExecuteTransactionCommon::L2(common_data) => Ok(L2Tx {
                execute,
                common_data,
                received_timestamp_ms,
                raw_bytes,
            }),
            ExecuteTransactionCommon::ProtocolUpgrade(_) => {
                Err("Cannot convert ProtocolUpgradeTx to L2Tx")
            }
        }
    }
}

impl EIP712TypedStructure for L2Tx {
    const TYPE_NAME: &'static str = "Transaction";

    fn build_structure<BUILDER: StructBuilder>(&self, builder: &mut BUILDER) {
        builder.add_member("txType", &(self.common_data.transaction_type as u8));

        self.execute.build_structure(builder);

        builder.add_member("gasLimit", &self.common_data.fee.gas_limit);
        builder.add_member(
            "gasPerPubdataByteLimit",
            &self.common_data.fee.gas_per_pubdata_limit,
        );
        builder.add_member("maxFeePerGas", &self.common_data.fee.max_fee_per_gas);
        builder.add_member(
            "maxPriorityFeePerGas",
            &self.common_data.fee.max_priority_fee_per_gas,
        );
        builder.add_member("nonce", &U256::from(self.common_data.nonce.0));
    }
}

#[cfg(test)]
mod tests {
    use zksync_basic_types::{Nonce, U256};

    use crate::{
        api::TransactionRequest, fee::Fee, transaction_request::PaymasterParams, Execute,
        L2TxCommonData,
    };

    use super::{L2Tx, TransactionType};

    #[test]
    fn test_correct_l2_tx_transaction_request_conversion() {
        // It is a random valid signature
        let signature = hex::decode("641ab4a25c4a209e0c80eb9ac81355f2dbe28d652ea9f5971fca60c8436096da0efda9d01346eaee9fb6234895eaf78bae0a42bd921a4ae3603b93489e19ed211b").unwrap();
        let tx = L2Tx {
            execute: Execute {
                contract_address: Default::default(),
                calldata: vec![],
                value: U256::zero(),
                factory_deps: None,
            },
            common_data: L2TxCommonData {
                nonce: Nonce(0),
                fee: Fee::default(),
                initiator_address: Default::default(),
                signature: signature.clone(),
                transaction_type: TransactionType::LegacyTransaction,
                input: None,
                paymaster_params: PaymasterParams::default(),
            },
            received_timestamp_ms: Default::default(),
            raw_bytes: None,
        };

        let transaction_request: TransactionRequest = tx.into();
        assert_eq!(
            transaction_request.v.unwrap().as_u32(),
            signature[64] as u32
        );
    }
}
