use std::convert::{TryFrom, TryInto};

use rlp::{DecoderError, Rlp, RlpStream};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zksync_system_constants::{DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE, MAX_ENCODED_TX_SIZE};

use super::{EIP_1559_TX_TYPE, EIP_2930_TX_TYPE, EIP_712_TX_TYPE};
use crate::{
    bytecode::{validate_bytecode, BytecodeHash, InvalidBytecodeError},
    fee::Fee,
    l1::L1Tx,
    l2::{L2Tx, TransactionType},
    u256_to_h256,
    web3::{
        keccak256, keccak256_concat, AccessList, AuthorizationList, AuthorizationListItem, Bytes,
    },
    Address, EIP712TypedStructure, Eip712Domain, L1TxCommonData, L2ChainId, Nonce,
    PackedEthSignature, StructBuilder, EIP_7702_TX_TYPE, H256, LEGACY_TX_TYPE, U256, U64,
};

/// Call contract request (eth_call / eth_estimateGas)
///
/// When using this for `eth_estimateGas`, all the fields
/// are optional. However, for usage in `eth_call` the
/// `to` field must be provided.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallRequest {
    /// Sender address (None for arbitrary address)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    /// To address (None allowed for eth_estimateGas)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Address>,
    /// Supplied gas (None for sensible default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gas: Option<U256>,
    /// Gas price (None for sensible default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gas_price: Option<U256>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_fee_per_gas: Option<U256>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_priority_fee_per_gas: Option<U256>,
    /// Transferred value (None for no transfer)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<U256>,
    /// Data (None for empty data)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Bytes>,
    /// Input (None for empty)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Bytes>,
    /// Nonce
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<U256>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub transaction_type: Option<U64>,
    /// Access list
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_list: Option<AccessList>,
    /// Authorization list
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_list: Option<AuthorizationList>,
    /// EIP712 meta
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eip712_meta: Option<Eip712Meta>,
}

/// While some default parameters are usually provided for the `eth_call` methods,
/// sometimes users may want to override those.
pub struct CallOverrides {
    pub enforced_base_fee: Option<u64>,
}

impl CallRequest {
    /// Function to return a builder for a Call Request
    pub fn builder() -> CallRequestBuilder {
        CallRequestBuilder::default()
    }

    pub fn get_call_overrides(&self) -> Result<CallOverrides, SerializationTransactionError> {
        let provided_gas_price = self.max_fee_per_gas.or(self.gas_price);
        let enforced_base_fee = if let Some(provided_gas_price) = provided_gas_price {
            Some(
                provided_gas_price
                    .try_into()
                    .map_err(|_| SerializationTransactionError::MaxFeePerGasNotU64)?,
            )
        } else {
            None
        };

        Ok(CallOverrides { enforced_base_fee })
    }
}

/// Call Request Builder
#[derive(Clone, Debug, Default)]
pub struct CallRequestBuilder {
    call_request: CallRequest,
}

impl CallRequestBuilder {
    /// Set sender address (None for arbitrary address)
    pub fn from(mut self, from: Address) -> Self {
        self.call_request.from = Some(from);
        self
    }

    /// Set to address (None allowed for eth_estimateGas)
    pub fn to(mut self, to: Option<Address>) -> Self {
        self.call_request.to = to;
        self
    }

    /// Set supplied gas (None for sensible default)
    pub fn gas(mut self, gas: U256) -> Self {
        self.call_request.gas = Some(gas);
        self
    }

    /// Set transferred, value (None for no transfer)
    pub fn gas_price(mut self, gas_price: U256) -> Self {
        self.call_request.gas_price = Some(gas_price);
        self
    }

    pub fn max_fee_per_gas(mut self, max_fee_per_gas: U256) -> Self {
        self.call_request.max_fee_per_gas = Some(max_fee_per_gas);
        self
    }

    pub fn max_priority_fee_per_gas(mut self, max_priority_fee_per_gas: U256) -> Self {
        self.call_request.max_priority_fee_per_gas = Some(max_priority_fee_per_gas);
        self
    }

    /// Set transferred, value (None for no transfer)
    pub fn value(mut self, value: U256) -> Self {
        self.call_request.value = Some(value);
        self
    }

    /// Set data (None for empty data)
    pub fn data(mut self, data: Bytes) -> Self {
        self.call_request.data = Some(data);
        self
    }

    pub fn input(mut self, input: Bytes) -> Self {
        self.call_request.input = Some(input);
        self
    }

    /// Set transaction type, Some(1) for AccessList transaction, None for Legacy
    pub fn transaction_type(mut self, transaction_type: U64) -> Self {
        self.call_request.transaction_type = Some(transaction_type);
        self
    }

    /// Set access list
    pub fn access_list(mut self, access_list: AccessList) -> Self {
        self.call_request.access_list = Some(access_list);
        self
    }

    /// Set meta
    pub fn eip712_meta(mut self, eip712_meta: Eip712Meta) -> Self {
        self.call_request.eip712_meta = Some(eip712_meta);
        self
    }

    /// build the Call Request
    pub fn build(&self) -> CallRequest {
        self.call_request.clone()
    }
}

#[derive(Debug, Error)]
pub enum SerializationTransactionError {
    #[error("transaction type is not supported")]
    UnknownTransactionFormat,
    #[error("toAddressIsNull")]
    ToAddressIsNull,
    #[error("incompleteSignature")]
    IncompleteSignature,
    #[error("fromAddressIsNull")]
    FromAddressIsNull,
    #[error("priceLimitToLow")]
    PriceLimitToLow,
    #[error("wrongToken")]
    WrongToken,
    #[error("decodeRlpError {0}")]
    DecodeRlpError(#[from] DecoderError),
    #[error("invalid signature")]
    MalformedSignature,
    #[error("malformed authorization list item with index {0}")]
    MalformedAuthorizationListItem(usize),
    #[error("wrong chain id {}", .0.unwrap_or_default())]
    WrongChainId(Option<u64>),
    #[error("malformed paymaster params")]
    MalforedPaymasterParams,
    #[error("factory dependency #{0} is invalid: {1}")]
    InvalidFactoryDependencies(usize, InvalidBytecodeError),
    #[error("access lists are not supported")]
    AccessListsNotSupported,
    #[error("nonce has max value")]
    TooBigNonce,

    /// Sanity checks to avoid extremely big numbers specified
    /// to gas and pubdata price.
    #[error("max fee per gas higher than 2^64-1")]
    MaxFeePerGasNotU64,
    #[error("max fee per pubdata byte higher than 2^64-1")]
    MaxFeePerPubdataByteNotU64,
    #[error("max priority fee per gas higher than 2^64-1")]
    MaxPriorityFeePerGasNotU64,

    /// OversizedData is returned if the raw tx size is greater
    /// than some meaningful limit a user might use. This is not a consensus error
    /// making the transaction invalid, rather a DOS protection.
    #[error("oversized data. max: {0}; actual: {1}")]
    OversizedData(usize, usize),
    #[error("gas per pub data limit is zero")]
    GasPerPubDataLimitZero,
}

#[derive(Clone, Debug, PartialEq, Default)]
/// Description of a Transaction, pending or in the chain.
pub struct TransactionRequest {
    /// Nonce
    pub nonce: U256,
    pub from: Option<Address>,
    /// Recipient (None when contract creation)
    pub to: Option<Address>,
    /// Transferred value
    pub value: U256,
    /// Gas Price
    pub gas_price: U256,
    /// Gas amount
    pub gas: U256,
    /// EIP-1559 part of gas price that goes to miners
    pub max_priority_fee_per_gas: Option<U256>,
    /// Input data
    pub input: Bytes,
    /// ECDSA recovery id
    pub v: Option<U64>,
    /// ECDSA signature r, 32 bytes
    pub r: Option<U256>,
    /// ECDSA signature s, 32 bytes
    pub s: Option<U256>,
    /// Raw transaction data
    pub raw: Option<Bytes>,
    /// Transaction type, Some(1) for AccessList transaction, None for Legacy
    pub transaction_type: Option<U64>,
    /// Access list
    pub access_list: Option<AccessList>,
    /// Authorization list (EIP-7702)
    pub authorization_list: Option<AuthorizationList>,
    pub eip712_meta: Option<Eip712Meta>,
    /// Chain ID
    pub chain_id: Option<u64>,
}

#[derive(Default, Serialize, Deserialize, Clone, PartialEq, Debug, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PaymasterParams {
    pub paymaster: Address,
    pub paymaster_input: Vec<u8>,
}

impl PaymasterParams {
    fn from_vector(value: Vec<Vec<u8>>) -> Result<Option<Self>, SerializationTransactionError> {
        if value.is_empty() {
            return Ok(None);
        }
        if value.len() != 2 || value[0].len() != 20 {
            return Err(SerializationTransactionError::MalforedPaymasterParams);
        }

        let result = Some(Self {
            paymaster: Address::from_slice(&value[0]),
            paymaster_input: value[1].clone(),
        });

        Ok(result)
    }
}

#[derive(Default, Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Eip712Meta {
    pub gas_per_pubdata: U256,
    #[serde(default)]
    pub factory_deps: Vec<Vec<u8>>,
    pub custom_signature: Option<Vec<u8>>,
    pub paymaster_params: Option<PaymasterParams>,
}

impl Eip712Meta {
    pub fn rlp_append(&self, rlp: &mut RlpStream) {
        rlp.append(&self.gas_per_pubdata);
        rlp.begin_list(self.factory_deps.len());
        for dep in &self.factory_deps {
            rlp.append(&dep.as_slice());
        }

        rlp_opt(rlp, &self.custom_signature);

        if let Some(paymaster_params) = &self.paymaster_params {
            rlp.begin_list(2);
            rlp.append(&paymaster_params.paymaster.as_bytes());
            rlp.append(&paymaster_params.paymaster_input);
        } else {
            rlp.begin_list(0);
        }
    }
}

impl EIP712TypedStructure for TransactionRequest {
    const TYPE_NAME: &'static str = "Transaction";

    fn build_structure<BUILDER: StructBuilder>(&self, builder: &mut BUILDER) {
        // We would prefer to return an error here, but the interface doesn't support that
        // and we need to enforce that the authorization list is not provided for EIP-712 transactions.
        assert!(
            self.authorization_list.is_none()
                || self
                    .authorization_list
                    .as_ref()
                    .map(|l| l.is_empty())
                    .unwrap_or(true),
            "Authorization list is not supported for EIP-712 txs"
        );

        let meta = self
            .eip712_meta
            .as_ref()
            .expect("We can sign transaction only with meta");
        builder.add_member(
            "txType",
            &self
                .transaction_type
                .map(|x| U256::from(x.as_u64()))
                .unwrap_or_else(|| U256::from(EIP_712_TX_TYPE)),
        );
        builder.add_member(
            "from",
            &U256::from(
                self.from
                    .expect("We can only sign transactions with known sender")
                    .as_bytes(),
            ),
        );
        builder.add_member("to", &U256::from(self.to.unwrap_or_default().as_bytes()));
        builder.add_member("gasLimit", &self.gas);
        builder.add_member("gasPerPubdataByteLimit", &meta.gas_per_pubdata);
        builder.add_member("maxFeePerGas", &self.gas_price);
        builder.add_member(
            "maxPriorityFeePerGas",
            &self.max_priority_fee_per_gas.unwrap_or(self.gas_price),
        );
        builder.add_member(
            "paymaster",
            &U256::from(self.get_paymaster().unwrap_or_default().as_bytes()),
        );
        builder.add_member("nonce", &self.nonce);
        builder.add_member("value", &self.value);
        builder.add_member("data", &self.input.0.as_slice());

        let factory_dep_hashes: Vec<_> = self
            .get_factory_deps()
            .into_iter()
            .map(|dep| BytecodeHash::for_bytecode(&dep).value())
            .collect();
        builder.add_member("factoryDeps", &factory_dep_hashes.as_slice());

        builder.add_member(
            "paymasterInput",
            &self.get_paymaster_input().unwrap_or_default().as_slice(),
        );
    }
}

impl TransactionRequest {
    pub fn get_custom_signature(&self) -> Option<Vec<u8>> {
        self.eip712_meta.as_ref()?.custom_signature.clone()
    }

    pub fn get_paymaster(&self) -> Option<Address> {
        Some(
            self.eip712_meta
                .as_ref()?
                .paymaster_params
                .as_ref()?
                .paymaster,
        )
    }

    pub fn get_paymaster_input(&self) -> Option<Vec<u8>> {
        Some(
            self.eip712_meta
                .as_ref()?
                .paymaster_params
                .as_ref()?
                .paymaster_input
                .clone(),
        )
    }

    pub fn get_factory_deps(&self) -> Vec<Vec<u8>> {
        self.eip712_meta
            .as_ref()
            .map(|meta| meta.factory_deps.clone())
            .unwrap_or_default()
    }

    // returns packed eth signature if it is present
    pub fn get_packed_signature(
        &self,
    ) -> Result<PackedEthSignature, SerializationTransactionError> {
        let packed_v = self
            .v
            .ok_or(SerializationTransactionError::IncompleteSignature)?
            .as_u64();
        let v = if !self.is_legacy_tx() {
            packed_v
                .try_into()
                .map_err(|_| SerializationTransactionError::MalformedSignature)?
        } else {
            let (v, _) = PackedEthSignature::unpack_v(packed_v)
                .map_err(|_| SerializationTransactionError::MalformedSignature)?;
            v
        };

        let packed_eth_signature = PackedEthSignature::from_rsv(
            &u256_to_h256(
                self.r
                    .ok_or(SerializationTransactionError::IncompleteSignature)?,
            ),
            &u256_to_h256(
                self.s
                    .ok_or(SerializationTransactionError::IncompleteSignature)?,
            ),
            v,
        );

        Ok(packed_eth_signature)
    }

    pub fn get_signature(&self) -> Result<Vec<u8>, SerializationTransactionError> {
        let custom_signature = self.get_custom_signature();
        if let Some(custom_sig) = custom_signature {
            // TODO (SMA-1584): Support empty signatures for accounts.
            if !custom_sig.is_empty() {
                // There was a custom signature supplied, it overrides
                // the v/r/s signature
                return Ok(custom_sig);
            }
        }

        let packed_eth_signature = self.get_packed_signature()?;

        Ok(packed_eth_signature.serialize_packed().to_vec())
    }

    pub fn get_signed_bytes(
        &self,
        signature: &PackedEthSignature,
    ) -> Result<Vec<u8>, SerializationTransactionError> {
        let mut rlp = RlpStream::new();
        self.rlp(&mut rlp, Some(signature))?;
        let mut data = rlp.out().to_vec();
        if let Some(tx_type) = self.transaction_type {
            data.insert(0, tx_type.as_u64() as u8);
        }
        Ok(data)
    }

    pub fn is_legacy_tx(&self) -> bool {
        self.transaction_type.is_none() || self.transaction_type == Some(LEGACY_TX_TYPE.into())
    }

    /// Encodes `TransactionRequest` to RLP.
    /// It may fail if `chain_id` is `None` while required.
    pub fn get_rlp(&self) -> Result<Vec<u8>, SerializationTransactionError> {
        let mut rlp_stream = RlpStream::new();
        self.rlp(&mut rlp_stream, None)?;
        Ok(rlp_stream.as_raw().into())
    }

    /// Encodes `TransactionRequest` to RLP.
    /// It may fail if `chain_id` is `None` while required.
    pub fn rlp(
        &self,
        rlp: &mut RlpStream,
        signature: Option<&PackedEthSignature>,
    ) -> Result<(), SerializationTransactionError> {
        rlp.begin_unbounded_list();

        match self.transaction_type {
            // EIP-2930 (0x01)
            Some(x) if x == EIP_2930_TX_TYPE.into() => {
                rlp.append(
                    &self
                        .chain_id
                        .ok_or(SerializationTransactionError::WrongChainId(None))?,
                );
                rlp.append(&self.nonce);
                rlp.append(&self.gas_price);
                rlp.append(&self.gas);
                rlp_opt(rlp, &self.to);
                rlp.append(&self.value);
                rlp.append(&self.input.0);
                access_list_rlp(rlp, &self.access_list);
            }
            // EIP-1559 (0x02)
            Some(x) if x == EIP_1559_TX_TYPE.into() => {
                rlp.append(
                    &self
                        .chain_id
                        .ok_or(SerializationTransactionError::WrongChainId(None))?,
                );
                rlp.append(&self.nonce);
                rlp_opt(rlp, &self.max_priority_fee_per_gas);
                rlp.append(&self.gas_price);
                rlp.append(&self.gas);
                rlp_opt(rlp, &self.to);
                rlp.append(&self.value);
                rlp.append(&self.input.0);
                access_list_rlp(rlp, &self.access_list);
            }
            // EIP-7702 (0x04)
            Some(x) if x == EIP_7702_TX_TYPE.into() => {
                // TODO: Should we add more safety rails, e.g. ensure that EIP712 meta is not provided?

                rlp.append(
                    &self
                        .chain_id
                        .ok_or(SerializationTransactionError::WrongChainId(None))?,
                );
                rlp.append(&self.nonce);
                rlp_opt(rlp, &self.max_priority_fee_per_gas);
                rlp.append(&self.gas_price);
                rlp.append(&self.gas);
                // To cannot be `null` for EIP-7702 transactions.
                let to = self
                    .to
                    .ok_or(SerializationTransactionError::ToAddressIsNull)?;
                rlp.append(&to);
                rlp.append(&self.value);
                rlp.append(&self.input.0);
                access_list_rlp(rlp, &self.access_list);
                authorization_list_rlp(rlp, &self.authorization_list);
            }
            // EIP-712
            Some(x) if x == EIP_712_TX_TYPE.into() => {
                rlp.append(&self.nonce);
                rlp_opt(rlp, &self.max_priority_fee_per_gas);
                rlp.append(&self.gas_price);
                rlp.append(&self.gas);
                rlp_opt(rlp, &self.to);
                rlp.append(&self.value);
                rlp.append(&self.input.0);
            }
            Some(x) if x == LEGACY_TX_TYPE.into() => {
                rlp.append(&self.nonce);
                rlp.append(&self.gas_price);
                rlp.append(&self.gas);
                rlp_opt(rlp, &self.to);
                rlp.append(&self.value);
                rlp.append(&self.input.0);
            }
            // Legacy (None)
            None => {
                rlp.append(&self.nonce);
                rlp.append(&self.gas_price);
                rlp.append(&self.gas);
                rlp_opt(rlp, &self.to);
                rlp.append(&self.value);
                rlp.append(&self.input.0);
            }
            Some(_) => unreachable!("Unknown tx type"),
        }

        match (signature, self.chain_id, self.is_legacy_tx()) {
            (Some(sig), Some(chain_id), true) => {
                rlp.append(&sig.v_with_chain_id(chain_id));
                rlp.append(&U256::from_big_endian(sig.r()));
                rlp.append(&U256::from_big_endian(sig.s()));
            }
            (None, Some(chain_id), true) => {
                rlp.append(&chain_id);
                rlp.append(&0u8);
                rlp.append(&0u8);
            }
            (Some(sig), _, _) => {
                rlp.append(&sig.v());
                rlp.append(&U256::from_big_endian(sig.r()));
                rlp.append(&U256::from_big_endian(sig.s()));
            }
            (None, _, _) => {}
        }

        if self.is_eip712_tx() {
            rlp.append(
                &self
                    .chain_id
                    .ok_or(SerializationTransactionError::WrongChainId(None))?,
            );
            rlp_opt(rlp, &self.from);
            if let Some(meta) = &self.eip712_meta {
                meta.rlp_append(rlp);
            }
        }

        rlp.finalize_unbounded_list();
        assert!(rlp.is_finished(), "RLP encoding is not finished");
        Ok(())
    }

    pub fn set_signature(&mut self, signature: &PackedEthSignature) {
        self.r = Some(U256::from_big_endian(signature.r()));
        self.s = Some(U256::from_big_endian(signature.s()));
        self.v = Some(signature.v().into())
    }

    fn decode_standard_fields(rlp: &Rlp, offset: usize) -> Result<Self, DecoderError> {
        Ok(Self {
            nonce: rlp.val_at(offset)?,
            gas_price: rlp.val_at(offset + 1)?,
            gas: rlp.val_at(offset + 2)?,
            to: rlp.val_at(offset + 3).ok(),
            value: rlp.val_at(offset + 4)?,
            input: Bytes(rlp.val_at(offset + 5)?),
            ..Default::default()
        })
    }

    fn decode_eip1559_fields(rlp: &Rlp, offset: usize) -> Result<Self, DecoderError> {
        Ok(Self {
            nonce: rlp.val_at(offset)?,
            max_priority_fee_per_gas: rlp.val_at(offset + 1).ok(),
            gas_price: rlp.val_at(offset + 2)?,
            gas: rlp.val_at(offset + 3)?,
            to: rlp.val_at(offset + 4).ok(),
            value: rlp.val_at(offset + 5)?,
            input: Bytes(rlp.val_at(offset + 6)?),
            ..Default::default()
        })
    }

    pub fn is_eip712_tx(&self) -> bool {
        Some(EIP_712_TX_TYPE.into()) == self.transaction_type
    }

    pub fn from_bytes_unverified(
        bytes: &[u8],
    ) -> Result<(Self, H256), SerializationTransactionError> {
        let rlp;
        let mut tx = match bytes.first() {
            Some(x) if *x >= 0x80 => {
                rlp = Rlp::new(bytes);
                if rlp.item_count()? != 9 {
                    return Err(DecoderError::RlpIncorrectListLen.into());
                }
                let v = rlp.val_at(6)?;
                Self {
                    // For legacy transactions `chain_id` is optional.
                    chain_id: PackedEthSignature::unpack_v(v)
                        .map_err(|_| SerializationTransactionError::MalformedSignature)?
                        .1,
                    v: Some(rlp.val_at(6)?),
                    r: Some(rlp.val_at(7)?),
                    s: Some(rlp.val_at(8)?),
                    ..Self::decode_standard_fields(&rlp, 0)?
                }
            }
            Some(&EIP_1559_TX_TYPE) => {
                rlp = Rlp::new(&bytes[1..]);
                if rlp.item_count()? != 12 {
                    return Err(DecoderError::RlpIncorrectListLen.into());
                }
                if let Ok(access_list_rlp) = rlp.at(8) {
                    if access_list_rlp.item_count()? > 0 {
                        return Err(SerializationTransactionError::AccessListsNotSupported);
                    }
                }
                Self {
                    chain_id: Some(rlp.val_at(0)?),
                    v: Some(rlp.val_at(9)?),
                    r: Some(rlp.val_at(10)?),
                    s: Some(rlp.val_at(11)?),
                    raw: Some(Bytes(rlp.as_raw().to_vec())),
                    transaction_type: Some(EIP_1559_TX_TYPE.into()),
                    ..Self::decode_eip1559_fields(&rlp, 1)?
                }
            }
            Some(&EIP_7702_TX_TYPE) => {
                rlp = Rlp::new(&bytes[1..]);
                if rlp.item_count()? != 13 {
                    return Err(DecoderError::RlpIncorrectListLen.into());
                }
                rlp.item_count()?;
                if let Ok(access_list_rlp) = rlp.at(8) {
                    if access_list_rlp.item_count()? > 0 {
                        return Err(SerializationTransactionError::AccessListsNotSupported);
                    }
                }
                let authorization_list_rlp = rlp.at(9)?;
                let authorization_list_item_count = authorization_list_rlp.item_count()?;
                let mut authorization_list = Vec::with_capacity(authorization_list_item_count);
                for i in 0..authorization_list_item_count {
                    let item = authorization_list_rlp.at(i)?;
                    if item.item_count()? != 6 {
                        return Err(
                            SerializationTransactionError::MalformedAuthorizationListItem(i),
                        );
                    }
                    let chain_id = item.val_at(0)?;
                    let address = item.val_at(1)?;
                    let nonce = item.val_at(2)?;
                    let y_parity = item.val_at(3)?;
                    let r = item.val_at(4)?;
                    let s = item.val_at(5)?;
                    let item = AuthorizationListItem {
                        chain_id,
                        address,
                        nonce,
                        y_parity,
                        r,
                        s,
                    };
                    authorization_list.push(item);
                }

                Self {
                    chain_id: Some(rlp.val_at(0)?),
                    v: Some(rlp.val_at(10)?),
                    r: Some(rlp.val_at(11)?),
                    s: Some(rlp.val_at(12)?),
                    raw: Some(Bytes(rlp.as_raw().to_vec())),
                    transaction_type: Some(EIP_7702_TX_TYPE.into()),
                    authorization_list: Some(authorization_list),
                    ..Self::decode_eip1559_fields(&rlp, 1)?
                }
            }
            Some(&EIP_712_TX_TYPE) => {
                rlp = Rlp::new(&bytes[1..]);
                if rlp.item_count()? != 16 {
                    return Err(DecoderError::RlpIncorrectListLen.into());
                }
                Self {
                    v: Some(rlp.val_at(7)?),
                    r: Some(rlp.val_at(8)?),
                    s: Some(rlp.val_at(9)?),
                    eip712_meta: Some(Eip712Meta {
                        gas_per_pubdata: rlp.val_at(12)?,
                        factory_deps: rlp.list_at(13)?,
                        custom_signature: rlp.val_at(14).ok(),
                        paymaster_params: if let Ok(params) = rlp.list_at(15) {
                            PaymasterParams::from_vector(params)?
                        } else {
                            None
                        },
                    }),
                    chain_id: Some(rlp.val_at(10)?),
                    transaction_type: Some(EIP_712_TX_TYPE.into()),
                    from: Some(rlp.val_at(11)?),
                    ..Self::decode_eip1559_fields(&rlp, 0)?
                }
            }
            Some(&EIP_2930_TX_TYPE) => {
                return Err(SerializationTransactionError::AccessListsNotSupported)
            }
            _ => return Err(SerializationTransactionError::UnknownTransactionFormat),
        };
        if let Some(meta) = &tx.eip712_meta {
            validate_factory_deps(&meta.factory_deps)?;
        }
        tx.raw = Some(Bytes(bytes.to_vec()));

        let default_signed_message = tx.get_default_signed_message()?;

        if tx.from.is_none() {
            tx.from = tx.recover_default_signer(default_signed_message).ok();
        }

        // `tx.raw` is set, so unwrap is safe here.
        let hash = tx
            .get_tx_hash_with_signed_message(default_signed_message)?
            .unwrap();
        Ok((tx, hash))
    }

    pub fn from_bytes(
        bytes: &[u8],
        chain_id: L2ChainId,
    ) -> Result<(Self, H256), SerializationTransactionError> {
        let (tx, hash) = Self::from_bytes_unverified(bytes)?;
        if tx.chain_id.is_some() && tx.chain_id != Some(chain_id.as_u64()) {
            return Err(SerializationTransactionError::WrongChainId(tx.chain_id));
        }
        Ok((tx, hash))
    }

    pub fn get_default_signed_message(&self) -> Result<H256, SerializationTransactionError> {
        if self.is_eip712_tx() {
            let chain_id = self
                .chain_id
                .ok_or(SerializationTransactionError::WrongChainId(None))?;
            Ok(PackedEthSignature::typed_data_to_signed_bytes(
                &Eip712Domain::new(L2ChainId::try_from(chain_id).unwrap()),
                self,
            ))
        } else {
            let mut data = self.get_rlp()?;
            if let Some(tx_type) = self.transaction_type {
                data.insert(0, tx_type.as_u64() as u8);
            }
            Ok(PackedEthSignature::message_to_signed_bytes(&data))
        }
    }

    fn get_tx_hash_with_signed_message(
        &self,
        signed_message: H256,
    ) -> Result<Option<H256>, SerializationTransactionError> {
        if self.is_eip712_tx() {
            return Ok(Some(keccak256_concat(
                signed_message,
                H256(keccak256(&self.get_signature()?)),
            )));
        }
        Ok(self.raw.as_ref().map(|bytes| H256(keccak256(&bytes.0))))
    }

    pub fn get_tx_hash(&self) -> Result<H256, SerializationTransactionError> {
        Ok(self.get_signed_and_tx_hashes()?.1)
    }

    pub fn get_signed_and_tx_hashes(&self) -> Result<(H256, H256), SerializationTransactionError> {
        let signed_message = self.get_default_signed_message()?;
        if let Some(tx_hash) = self.get_tx_hash_with_signed_message(signed_message)? {
            return Ok((signed_message, tx_hash));
        }
        let signature = self.get_packed_signature()?;
        let tx_hash = H256(keccak256(&self.get_signed_bytes(&signature)?));
        Ok((signed_message, tx_hash))
    }

    fn recover_default_signer(
        &self,
        default_signed_message: H256,
    ) -> Result<Address, SerializationTransactionError> {
        let signature = self.get_signature()?;

        let address = PackedEthSignature::deserialize_packed(&signature)
            .map_err(|_| SerializationTransactionError::MalformedSignature)?
            .signature_recover_signer(&default_signed_message)
            .map_err(|_| SerializationTransactionError::MalformedSignature)?;

        Ok(address)
    }

    fn get_fee_data_checked(&self) -> Result<Fee, SerializationTransactionError> {
        if self.gas_price > u64::MAX.into() {
            return Err(SerializationTransactionError::MaxFeePerGasNotU64);
        }

        let gas_per_pubdata_limit = if let Some(meta) = &self.eip712_meta {
            if meta.gas_per_pubdata > u64::MAX.into() {
                return Err(SerializationTransactionError::MaxFeePerPubdataByteNotU64);
            } else if meta.gas_per_pubdata == U256::zero() {
                return Err(SerializationTransactionError::GasPerPubDataLimitZero);
            }
            meta.gas_per_pubdata
        } else {
            // For transactions that don't support corresponding field, a maximal default value is chosen.
            DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE.into()
        };

        let max_priority_fee_per_gas = self.max_priority_fee_per_gas.unwrap_or(self.gas_price);
        if max_priority_fee_per_gas > u64::MAX.into() {
            return Err(SerializationTransactionError::MaxPriorityFeePerGasNotU64);
        }

        Ok(Fee {
            gas_limit: self.gas,
            max_fee_per_gas: self.gas_price,
            max_priority_fee_per_gas,
            gas_per_pubdata_limit,
        })
    }

    fn get_nonce_checked(&self) -> Result<Nonce, SerializationTransactionError> {
        if self.nonce <= U256::from(u32::MAX) {
            Ok(Nonce(self.nonce.as_u32()))
        } else {
            Err(SerializationTransactionError::TooBigNonce)
        }
    }
}

impl L2Tx {
    pub(crate) fn from_request_unverified(
        mut value: TransactionRequest,
        allow_no_target: bool,
    ) -> Result<Self, SerializationTransactionError> {
        let fee = value.get_fee_data_checked()?;
        let nonce = value.get_nonce_checked()?;

        let raw_signature = value.get_signature().unwrap_or_default();
        let meta = value.eip712_meta.take().unwrap_or_default();
        validate_factory_deps(&meta.factory_deps)?;

        if value.to.is_none() && (!allow_no_target || value.is_eip712_tx()) {
            return Err(SerializationTransactionError::ToAddressIsNull);
        }

        let mut tx = L2Tx::new(
            value.to,
            value.input.0.clone(),
            nonce,
            fee,
            value.from.unwrap_or_default(),
            value.value,
            meta.factory_deps,
            meta.paymaster_params.unwrap_or_default(),
        );

        tx.common_data.transaction_type = match value.transaction_type.map(|t| t.as_u64() as u8) {
            Some(EIP_712_TX_TYPE) => TransactionType::EIP712Transaction,
            Some(EIP_1559_TX_TYPE) => TransactionType::EIP1559Transaction,
            Some(EIP_2930_TX_TYPE) => TransactionType::EIP2930Transaction,
            Some(EIP_7702_TX_TYPE) => TransactionType::EIP7702Transaction,
            None if value.authorization_list.is_some() => TransactionType::EIP7702Transaction,
            _ => TransactionType::LegacyTransaction,
        };
        if tx.common_data.transaction_type == TransactionType::EIP7702Transaction {
            tx.common_data.authorization_list = value.authorization_list;
        }

        // For fee calculation we use the same structure, as a result, signature may not be provided
        tx.set_raw_signature(raw_signature);

        if let Some(raw_bytes) = value.raw {
            tx.set_raw_bytes(raw_bytes);
        }
        Ok(tx)
    }

    /// Converts a request into a transaction.
    ///
    /// # Arguments
    ///
    /// - `allow_no_target` enables / disables transactions without target (i.e., `to` field).
    ///   This field can only be absent for EVM deployment transactions.
    pub fn from_request(
        request: TransactionRequest,
        max_tx_size: usize,
        allow_no_target: bool,
    ) -> Result<Self, SerializationTransactionError> {
        let tx = Self::from_request_unverified(request, allow_no_target)?;
        tx.check_encoded_size(max_tx_size)?;
        Ok(tx)
    }

    /// Ensures that encoded transaction size is not greater than `max_tx_size`.
    fn check_encoded_size(&self, max_tx_size: usize) -> Result<(), SerializationTransactionError> {
        // since `abi_encoding_len` returns 32-byte words multiplication on 32 is needed
        let tx_size = self.abi_encoding_len() * 32;
        if tx_size > max_tx_size {
            return Err(SerializationTransactionError::OversizedData(
                max_tx_size,
                tx_size,
            ));
        };
        Ok(())
    }
}

impl From<L2Tx> for CallRequest {
    fn from(tx: L2Tx) -> Self {
        let mut meta = Eip712Meta {
            gas_per_pubdata: tx.common_data.fee.gas_per_pubdata_limit,
            factory_deps: vec![],
            custom_signature: Some(tx.common_data.signature.clone()),
            paymaster_params: Some(tx.common_data.paymaster_params.clone()),
        };
        meta.factory_deps.clone_from(&tx.execute.factory_deps);
        let mut request = CallRequestBuilder::default()
            .from(tx.initiator_account())
            .gas(tx.common_data.fee.gas_limit)
            .max_fee_per_gas(tx.common_data.fee.max_fee_per_gas)
            .max_priority_fee_per_gas(tx.common_data.fee.max_priority_fee_per_gas)
            .transaction_type(U64::from(tx.common_data.transaction_type as u32))
            .to(tx.execute.contract_address)
            .data(Bytes(tx.execute.calldata.clone()))
            .eip712_meta(meta)
            .build();

        if tx.common_data.transaction_type == TransactionType::LegacyTransaction {
            request.transaction_type = None;
        }
        request
    }
}

impl From<CallRequest> for TransactionRequest {
    fn from(call_request: CallRequest) -> Self {
        TransactionRequest {
            nonce: call_request.nonce.unwrap_or_default(),
            from: call_request.from,
            to: call_request.to,
            value: call_request.value.unwrap_or_default(),
            gas_price: call_request.gas_price.unwrap_or_default(),
            gas: call_request.gas.unwrap_or_default(),
            input: call_request.input.or(call_request.data).unwrap_or_default(),
            transaction_type: call_request.transaction_type,
            access_list: call_request.access_list,
            eip712_meta: call_request.eip712_meta,
            authorization_list: call_request.authorization_list,
            ..Default::default()
        }
    }
}

impl L1Tx {
    /// Converts a request into a transaction.
    ///
    /// # Arguments
    ///
    /// - `allow_no_target` enables / disables transactions without target (i.e., `to` field).
    ///   This field can only be absent for EVM deployment transactions.
    pub fn from_request(
        request: CallRequest,
        allow_no_target: bool,
    ) -> Result<Self, SerializationTransactionError> {
        // L1 transactions have no limitations on the transaction size.
        let tx: L2Tx = L2Tx::from_request(request.into(), MAX_ENCODED_TX_SIZE, allow_no_target)?;

        // Note, that while the user has theoretically provided the fee for ETH on L1,
        // the payment to the operator as well as refunds happen on L2 and so all the ETH
        // that the transaction requires to pay the operator needs to be minted on L2.
        let total_needed_eth =
            tx.execute.value + tx.common_data.fee.max_fee_per_gas * tx.common_data.fee.gas_limit;

        // Note, that we do not set `refund_recipient` here, to keep it explicitly 0,
        // so that during fee estimation it is taken into account that the refund recipient may be a different address
        let common_data = L1TxCommonData {
            sender: tx.common_data.initiator_address,
            max_fee_per_gas: tx.common_data.fee.max_fee_per_gas,
            gas_limit: tx.common_data.fee.gas_limit,
            gas_per_pubdata_limit: tx.common_data.fee.gas_per_pubdata_limit,
            to_mint: total_needed_eth,
            ..Default::default()
        };

        let tx = L1Tx {
            execute: tx.execute,
            common_data,
            received_timestamp_ms: 0u64,
        };

        Ok(tx)
    }
}

fn rlp_opt<T: rlp::Encodable>(rlp: &mut RlpStream, opt: &Option<T>) {
    if let Some(inner) = opt {
        rlp.append(inner);
    } else {
        rlp.append(&"");
    }
}

fn access_list_rlp(rlp: &mut RlpStream, access_list: &Option<AccessList>) {
    if let Some(access_list) = access_list {
        rlp.begin_list(access_list.len());
        for item in access_list {
            rlp.begin_list(2);
            rlp.append(&item.address);
            rlp.append_list(&item.storage_keys);
        }
    } else {
        rlp.begin_list(0);
    }
}

fn authorization_list_rlp(rlp: &mut RlpStream, authorization_list: &Option<AuthorizationList>) {
    if let Some(authorization_list) = authorization_list {
        rlp.begin_list(authorization_list.len());
        for item in authorization_list {
            rlp.begin_list(6);
            rlp.append(&item.chain_id);
            rlp.append(&item.address);
            rlp.append(&item.nonce);
            rlp.append(&item.y_parity);
            rlp.append(&item.r);
            rlp.append(&item.s);
        }
    } else {
        rlp.begin_list(0);
    }
}

pub fn validate_factory_deps(
    factory_deps: &[Vec<u8>],
) -> Result<(), SerializationTransactionError> {
    for (i, dep) in factory_deps.iter().enumerate() {
        validate_bytecode(dep)
            .map_err(|err| SerializationTransactionError::InvalidFactoryDependencies(i, err))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use zksync_crypto_primitives::K256PrivateKey;

    use super::*;

    #[test]
    fn decode_rlp() {
        let private_key = K256PrivateKey::random();
        let address = private_key.address();

        let mut tx = TransactionRequest {
            nonce: U256::from(1u32),
            to: Some(Address::random()),
            value: U256::from(10u32),
            gas_price: U256::from(11u32),
            gas: U256::from(12u32),
            input: Bytes::from(vec![1, 2, 3]),
            chain_id: Some(270),
            ..Default::default()
        };
        let mut rlp = RlpStream::new();
        tx.rlp(&mut rlp, None).unwrap();
        let data = rlp.out().to_vec();
        let msg = PackedEthSignature::message_to_signed_bytes(&data);
        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();
        tx.raw = Some(Bytes(data));
        let mut rlp = RlpStream::new();
        tx.rlp(&mut rlp, Some(&signature)).unwrap();
        let data = rlp.out().to_vec();
        let (tx2, _) = TransactionRequest::from_bytes(&data, L2ChainId::from(270)).unwrap();
        assert_eq!(tx.gas, tx2.gas);
        assert_eq!(tx.gas_price, tx2.gas_price);
        assert_eq!(tx.nonce, tx2.nonce);
        assert_eq!(tx.input, tx2.input);
        assert_eq!(tx.value, tx2.value);
        assert_eq!(tx2.v.unwrap().as_u64(), signature.v_with_chain_id(270));
        assert_eq!(tx2.s.unwrap(), signature.s().into());
        assert_eq!(tx2.r.unwrap(), signature.r().into());
        assert_eq!(address, tx2.from.unwrap());
    }

    #[test]
    fn decode_eip712_with_meta() {
        let private_key = K256PrivateKey::random();
        let address = private_key.address();

        let mut tx = TransactionRequest {
            nonce: U256::from(1u32),
            to: Some(Address::random()),
            from: Some(address),
            value: U256::from(10u32),
            gas_price: U256::from(11u32),
            max_priority_fee_per_gas: Some(U256::from(0u32)),
            gas: U256::from(12u32),
            input: Bytes::from(vec![1, 2, 3]),
            transaction_type: Some(U64::from(EIP_712_TX_TYPE)),
            eip712_meta: Some(Eip712Meta {
                gas_per_pubdata: U256::from(4u32),
                factory_deps: vec![vec![2; 32]],
                custom_signature: Some(vec![1, 2, 3]),
                paymaster_params: Some(PaymasterParams {
                    paymaster: Default::default(),
                    paymaster_input: vec![],
                }),
            }),
            chain_id: Some(270),
            ..Default::default()
        };

        let msg = PackedEthSignature::typed_data_to_signed_bytes(
            &Eip712Domain::new(L2ChainId::from(270)),
            &tx,
        );
        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();

        let mut rlp = RlpStream::new();
        tx.rlp(&mut rlp, Some(&signature)).unwrap();
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_712_TX_TYPE);
        tx.raw = Some(Bytes(data.clone()));
        tx.v = Some(U64::from(signature.v()));
        tx.r = Some(U256::from_big_endian(signature.r()));
        tx.s = Some(U256::from_big_endian(signature.s()));

        let (tx2, _) = TransactionRequest::from_bytes(&data, L2ChainId::from(270)).unwrap();

        assert_eq!(tx, tx2);
    }

    #[test]
    fn check_recovered_public_key_eip712() {
        let private_key = K256PrivateKey::random();
        let address = private_key.address();

        let transaction_request = TransactionRequest {
            nonce: U256::from(1u32),
            to: Some(Address::random()),
            from: Some(address),
            value: U256::from(10u32),
            gas_price: U256::from(11u32),
            max_priority_fee_per_gas: Some(U256::from(0u32)),
            gas: U256::from(12u32),
            input: Bytes::from(vec![1, 2, 3]),
            transaction_type: Some(U64::from(EIP_712_TX_TYPE)),
            eip712_meta: Some(Eip712Meta {
                gas_per_pubdata: U256::from(4u32),
                factory_deps: vec![vec![2; 32]],
                custom_signature: Some(vec![]),
                paymaster_params: None,
            }),
            chain_id: Some(270),
            ..Default::default()
        };
        let domain = Eip712Domain::new(L2ChainId::from(270));
        let signature =
            PackedEthSignature::sign_typed_data(&private_key, &domain, &transaction_request)
                .unwrap();

        let encoded_tx = transaction_request.get_signed_bytes(&signature).unwrap();

        let (decoded_tx, _) =
            TransactionRequest::from_bytes(encoded_tx.as_slice(), L2ChainId::from(270)).unwrap();
        let recovered_signer = decoded_tx.from.unwrap();
        assert_eq!(address, recovered_signer);
    }

    #[test]
    fn check_recovered_public_key_eip712_with_wrong_chain_id() {
        let private_key = K256PrivateKey::random();
        let address = private_key.address();

        let transaction_request = TransactionRequest {
            nonce: U256::from(1u32),
            to: Some(Address::random()),
            from: Some(address),
            value: U256::from(10u32),
            gas_price: U256::from(11u32),
            max_priority_fee_per_gas: Some(U256::from(0u32)),
            gas: U256::from(12u32),
            input: Bytes::from(vec![1, 2, 3]),
            transaction_type: Some(U64::from(EIP_712_TX_TYPE)),
            eip712_meta: Some(Eip712Meta {
                gas_per_pubdata: U256::from(4u32),
                factory_deps: vec![vec![2; 32]],
                custom_signature: Some(vec![1, 2, 3]),
                paymaster_params: Some(PaymasterParams {
                    paymaster: Default::default(),
                    paymaster_input: vec![],
                }),
            }),
            chain_id: Some(270),
            ..Default::default()
        };
        let domain = Eip712Domain::new(L2ChainId::from(270));
        let signature =
            PackedEthSignature::sign_typed_data(&private_key, &domain, &transaction_request)
                .unwrap();

        let encoded_tx = transaction_request.get_signed_bytes(&signature).unwrap();

        let decoded_tx =
            TransactionRequest::from_bytes(encoded_tx.as_slice(), L2ChainId::from(272));
        assert_matches!(
            decoded_tx.unwrap_err(),
            SerializationTransactionError::WrongChainId(Some(270))
        );
    }

    #[test]
    fn check_recovered_public_key_eip1559() {
        let private_key = K256PrivateKey::random();
        let address = private_key.address();

        let mut transaction_request = TransactionRequest {
            max_priority_fee_per_gas: Some(U256::from(1u32)),
            raw: None,
            transaction_type: Some(EIP_1559_TX_TYPE.into()),
            nonce: U256::from(1u32),
            to: Some(Address::random()),
            from: Some(address),
            value: U256::from(10u32),
            gas_price: U256::from(11u32),
            gas: U256::from(12u32),
            input: Bytes::from(vec![1, 2, 3]),
            chain_id: Some(270),
            access_list: Some(Vec::new()),
            ..Default::default()
        };
        let mut rlp_stream = RlpStream::new();
        transaction_request.rlp(&mut rlp_stream, None).unwrap();
        let mut data = rlp_stream.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);
        let msg = PackedEthSignature::message_to_signed_bytes(&data);

        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();
        transaction_request.raw = Some(Bytes(data));
        let mut rlp = RlpStream::new();
        transaction_request.rlp(&mut rlp, Some(&signature)).unwrap();
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);

        let (decoded_tx, _) =
            TransactionRequest::from_bytes(data.as_slice(), L2ChainId::from(270)).unwrap();
        let recovered_signer = decoded_tx.from.unwrap();
        assert_eq!(address, recovered_signer);
    }

    #[test]
    fn check_recovered_public_key_eip1559_with_wrong_chain_id() {
        let private_key = K256PrivateKey::random();
        let address = private_key.address();

        let mut transaction_request = TransactionRequest {
            max_priority_fee_per_gas: Some(U256::from(1u32)),
            raw: None,
            transaction_type: Some(EIP_1559_TX_TYPE.into()),
            nonce: U256::from(1u32),
            to: Some(Address::random()),
            from: Some(address),
            value: U256::from(10u32),
            gas_price: U256::from(11u32),
            gas: U256::from(12u32),
            input: Bytes::from(vec![1, 2, 3]),
            chain_id: Some(272),
            ..Default::default()
        };
        let mut rlp_stream = RlpStream::new();
        transaction_request.rlp(&mut rlp_stream, None).unwrap();
        let mut data = rlp_stream.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);
        let msg = PackedEthSignature::message_to_signed_bytes(&data);

        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();
        transaction_request.raw = Some(Bytes(data));
        let mut rlp = RlpStream::new();
        transaction_request.rlp(&mut rlp, Some(&signature)).unwrap();
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);

        let decoded_tx = TransactionRequest::from_bytes(data.as_slice(), L2ChainId::from(270));
        assert_matches!(
            decoded_tx.unwrap_err(),
            SerializationTransactionError::WrongChainId(Some(272))
        );
    }

    #[test]
    fn check_decode_eip1559_with_access_list() {
        let private_key = K256PrivateKey::random();
        let address = private_key.address();

        let mut transaction_request = TransactionRequest {
            max_priority_fee_per_gas: Some(U256::from(1u32)),
            raw: None,
            transaction_type: Some(EIP_1559_TX_TYPE.into()),
            nonce: U256::from(1u32),
            to: Some(Address::random()),
            from: Some(address),
            value: U256::from(10u32),
            gas_price: U256::from(11u32),
            gas: U256::from(12u32),
            input: Bytes::from(vec![1, 2, 3]),
            chain_id: Some(270),
            access_list: Some(vec![Default::default()]),
            ..Default::default()
        };
        let mut rlp_stream = RlpStream::new();
        transaction_request.rlp(&mut rlp_stream, None).unwrap();
        let mut data = rlp_stream.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);
        let msg = PackedEthSignature::message_to_signed_bytes(&data);

        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();
        transaction_request.raw = Some(Bytes(data));
        let mut rlp = RlpStream::new();
        transaction_request.rlp(&mut rlp, Some(&signature)).unwrap();
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);

        let res = TransactionRequest::from_bytes(data.as_slice(), L2ChainId::from(270));
        assert_matches!(
            res.unwrap_err(),
            SerializationTransactionError::AccessListsNotSupported
        );
    }

    #[test]
    fn check_failed_to_decode_eip2930() {
        let private_key = K256PrivateKey::random();
        let address = private_key.address();

        let mut transaction_request = TransactionRequest {
            transaction_type: Some(EIP_2930_TX_TYPE.into()),
            nonce: U256::from(1u32),
            to: Some(Address::random()),
            from: Some(address),
            value: U256::from(10u32),
            gas_price: U256::from(11u32),
            gas: U256::from(12u32),
            input: Bytes::from(vec![1, 2, 3]),
            chain_id: Some(270),
            ..Default::default()
        };
        let mut rlp_stream = RlpStream::new();
        transaction_request.rlp(&mut rlp_stream, None).unwrap();
        let mut data = rlp_stream.out().to_vec();
        data.insert(0, EIP_2930_TX_TYPE);
        let msg = PackedEthSignature::message_to_signed_bytes(&data);

        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();
        transaction_request.raw = Some(Bytes(data));
        let mut rlp = RlpStream::new();
        transaction_request.rlp(&mut rlp, Some(&signature)).unwrap();
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_2930_TX_TYPE);

        let res = TransactionRequest::from_bytes(data.as_slice(), L2ChainId::from(270));
        assert_matches!(
            res.unwrap_err(),
            SerializationTransactionError::AccessListsNotSupported
        );
    }

    #[test]
    fn check_transaction_request_big_nonce() {
        let tx1 = TransactionRequest {
            nonce: U256::from(u32::MAX),
            to: Some(Address::repeat_byte(0x1)),
            from: Some(Address::repeat_byte(0x1)),
            value: U256::zero(),
            ..Default::default()
        };
        let execute_tx1: Result<L2Tx, SerializationTransactionError> =
            L2Tx::from_request(tx1, usize::MAX, true);
        assert!(execute_tx1.is_ok());

        let tx2 = TransactionRequest {
            nonce: U256::from(u32::MAX as u64 + 1),
            to: Some(Address::repeat_byte(0x1)),
            from: Some(Address::repeat_byte(0x1)),
            value: U256::zero(),
            ..Default::default()
        };
        let execute_tx2: Result<L2Tx, SerializationTransactionError> =
            L2Tx::from_request(tx2, usize::MAX, true);
        assert_matches!(
            execute_tx2.unwrap_err(),
            SerializationTransactionError::TooBigNonce
        );
    }

    #[test]
    fn transaction_request_with_big_gas() {
        let tx1 = TransactionRequest {
            to: Some(Address::repeat_byte(0x1)),
            from: Some(Address::repeat_byte(0x1)),
            value: U256::zero(),
            gas_price: U256::MAX,
            ..Default::default()
        };
        let execute_tx1: Result<L2Tx, SerializationTransactionError> =
            L2Tx::from_request(tx1, usize::MAX, true);
        assert_matches!(
            execute_tx1.unwrap_err(),
            SerializationTransactionError::MaxFeePerGasNotU64
        );

        let tx2 = TransactionRequest {
            to: Some(Address::repeat_byte(0x1)),
            from: Some(Address::repeat_byte(0x1)),
            value: U256::zero(),
            max_priority_fee_per_gas: Some(U256::MAX),
            ..Default::default()
        };
        let execute_tx2: Result<L2Tx, SerializationTransactionError> =
            L2Tx::from_request(tx2, usize::MAX, true);
        assert_matches!(
            execute_tx2.unwrap_err(),
            SerializationTransactionError::MaxPriorityFeePerGasNotU64
        );

        let tx3 = TransactionRequest {
            to: Some(Address::repeat_byte(0x1)),
            from: Some(Address::repeat_byte(0x1)),
            value: U256::zero(),
            eip712_meta: Some(Eip712Meta {
                gas_per_pubdata: U256::MAX,
                ..Default::default()
            }),
            ..Default::default()
        };

        let execute_tx3: Result<L2Tx, SerializationTransactionError> =
            L2Tx::from_request(tx3, usize::MAX, true);
        assert_matches!(
            execute_tx3.unwrap_err(),
            SerializationTransactionError::MaxFeePerPubdataByteNotU64
        );
    }

    #[test]
    fn transaction_request_with_oversize_data() {
        let random_tx_max_size = 1_000_000; // bytes
        let private_key = K256PrivateKey::random();
        let address = private_key.address();

        // Choose some number that divides on 8 and is `> 1_000_000`
        let factory_dep = vec![2u8; 1600000];
        let factory_deps: Vec<Vec<u8>> = factory_dep.chunks(32).map(|s| s.into()).collect();
        let mut tx = TransactionRequest {
            nonce: U256::from(1u32),
            to: Some(Address::random()),
            from: Some(address),
            value: U256::from(10u32),
            gas_price: U256::from(11u32),
            max_priority_fee_per_gas: Some(U256::from(0u32)),
            gas: U256::from(12u32),
            input: Bytes::from(vec![1, 2, 3]),
            transaction_type: Some(U64::from(EIP_712_TX_TYPE)),
            eip712_meta: Some(Eip712Meta {
                gas_per_pubdata: U256::from(4u32),
                factory_deps,
                custom_signature: Some(vec![1, 2, 3]),
                paymaster_params: Some(PaymasterParams {
                    paymaster: Default::default(),
                    paymaster_input: vec![],
                }),
            }),
            chain_id: Some(270),
            ..Default::default()
        };

        let msg = PackedEthSignature::typed_data_to_signed_bytes(
            &Eip712Domain::new(L2ChainId::from(270)),
            &tx,
        );
        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();

        let mut rlp = RlpStream::new();
        tx.rlp(&mut rlp, Some(&signature)).unwrap();
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_712_TX_TYPE);
        tx.raw = Some(Bytes(data.clone()));
        tx.v = Some(U64::from(signature.v()));
        tx.r = Some(U256::from_big_endian(signature.r()));
        tx.s = Some(U256::from_big_endian(signature.s()));
        let request =
            TransactionRequest::from_bytes(data.as_slice(), L2ChainId::from(270)).unwrap();
        assert_matches!(
            L2Tx::from_request(request.0, random_tx_max_size, true),
            Err(SerializationTransactionError::OversizedData(_, _))
        )
    }

    #[test]
    fn check_call_req_to_l2_tx_oversize_data() {
        let calldata = vec![2u8; 1600000];
        let random_tx_max_size = 100_000; // bytes
        let call_request = CallRequest {
            from: Some(Address::random()),
            to: Some(Address::random()),
            gas: Some(U256::from(12u32)),
            gas_price: Some(U256::from(12u32)),
            max_fee_per_gas: Some(U256::from(12u32)),
            max_priority_fee_per_gas: Some(U256::from(12u32)),
            value: Some(U256::from(12u32)),
            data: Some(Bytes(calldata)),
            input: None,
            nonce: None,
            transaction_type: Some(U64::from(EIP_712_TX_TYPE)),
            access_list: None,
            authorization_list: None,
            eip712_meta: None,
        };

        let try_to_l2_tx: Result<L2Tx, SerializationTransactionError> =
            L2Tx::from_request(call_request.into(), random_tx_max_size, true);

        assert_matches!(
            try_to_l2_tx,
            Err(SerializationTransactionError::OversizedData(_, _))
        );
    }

    #[test]
    fn test_tx_req_from_call_req_nonce_pass() {
        let call_request_with_nonce = CallRequest {
            from: Some(Address::random()),
            to: Some(Address::random()),
            gas: Some(U256::from(12u32)),
            gas_price: Some(U256::from(12u32)),
            max_fee_per_gas: Some(U256::from(12u32)),
            max_priority_fee_per_gas: Some(U256::from(12u32)),
            value: Some(U256::from(12u32)),
            data: Some(Bytes(vec![1, 2, 3])),
            input: None,
            nonce: Some(U256::from(123u32)),
            transaction_type: Some(U64::from(EIP_712_TX_TYPE)),
            access_list: None,
            authorization_list: None,
            eip712_meta: None,
        };
        let l2_tx = L2Tx::from_request(
            call_request_with_nonce.clone().into(),
            MAX_ENCODED_TX_SIZE,
            true,
        )
        .unwrap();
        assert_eq!(l2_tx.nonce(), Nonce(123u32));

        let mut call_request_without_nonce = call_request_with_nonce;
        call_request_without_nonce.nonce = None;

        let l2_tx =
            L2Tx::from_request(call_request_without_nonce.into(), MAX_ENCODED_TX_SIZE, true)
                .unwrap();
        assert_eq!(l2_tx.nonce(), Nonce(0u32));
    }

    #[test]
    fn test_correct_data_field() {
        let mut call_request = CallRequest {
            input: Some(Bytes(vec![1, 2, 3])),
            data: Some(Bytes(vec![3, 2, 1])),
            ..Default::default()
        };

        let tx_request = TransactionRequest::from(call_request.clone());
        assert_eq!(tx_request.input, call_request.input.unwrap());

        call_request.input = None;
        let tx_request = TransactionRequest::from(call_request.clone());
        assert_eq!(tx_request.input, call_request.data.unwrap());

        call_request.input = Some(Bytes(vec![1, 2, 3]));
        call_request.data = None;
        let tx_request = TransactionRequest::from(call_request.clone());
        assert_eq!(tx_request.input, call_request.input.unwrap());
    }

    #[test]
    fn test_eip712_without_field_to() {
        let calldata = vec![2u8; 64];
        let random_tx_max_size = 100_000; // bytes
        let eip712_call_request = CallRequest {
            from: Some(Address::random()),
            to: None,
            gas: Some(U256::from(12u32)),
            gas_price: Some(U256::from(12u32)),
            max_fee_per_gas: Some(U256::from(12u32)),
            max_priority_fee_per_gas: Some(U256::from(12u32)),
            value: Some(U256::from(12u32)),
            data: Some(Bytes(calldata)),
            input: None,
            nonce: None,
            transaction_type: Some(U64::from(EIP_712_TX_TYPE)),
            access_list: None,
            authorization_list: None,
            eip712_meta: None,
        };

        let try_to_l2_tx: Result<L2Tx, SerializationTransactionError> =
            L2Tx::from_request(eip712_call_request.into(), random_tx_max_size, true);

        assert_matches!(
            try_to_l2_tx,
            Err(SerializationTransactionError::ToAddressIsNull)
        );
    }

    #[test]
    fn rlp_7702_roundtrip() {
        let rlp = hex::decode(
            "04f8cb8201048001840564eba18304f05a9400000000000000000000000000000000000080018203e880c0f85ef85c8201049400000000000000000000000000000000000080010180a09732937ba3de2800af90a73693b1d5c6171ab706c74c5c58092eebb0c8c83f2aa05f144b1a71680590a263eb7b22f66f3807334b01fba030feb1cb833a2f64a6a001a0929ff64f7063d6051748fb0cda1f87949625a012fa403eb302c42c0e0c550abba03955758dd3e964267eee258cdf02265819f8f293a3f0e1aca0e4a261af4a32a1"
        ).unwrap();
        let (tx, _hash) =
            TransactionRequest::from_bytes(rlp.as_slice(), L2ChainId::from(260)).unwrap();
        assert_eq!(tx.transaction_type, Some(EIP_7702_TX_TYPE.into()));
        let authorization_list = tx.authorization_list.clone().unwrap();
        assert_eq!(authorization_list.len(), 1);
        assert_eq!(
            authorization_list[0].address,
            Address::from_slice(
                hex::decode("0000000000000000000000000000000000008001")
                    .unwrap()
                    .as_slice()
            )
        );
        assert_eq!(authorization_list[0].nonce, 1.into());
        assert_eq!(authorization_list[0].chain_id, 260.into());

        let signature = tx.get_packed_signature().unwrap();
        let mut rlp_stream = RlpStream::new();
        tx.rlp(&mut rlp_stream, Some(&signature)).unwrap();
        let roundtrip_rlp = rlp_stream.as_raw();
        // // Split the first byte (tx type) from initial RLP.
        assert_eq!(&rlp[1..], roundtrip_rlp);
    }
}
