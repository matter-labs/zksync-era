// Built-in uses
use std::convert::{TryFrom, TryInto};

// External uses
use rlp::{DecoderError, Rlp, RlpStream};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zksync_basic_types::H256;

use zksync_system_constants::{MAX_GAS_PER_PUBDATA_BYTE, USED_BOOTLOADER_MEMORY_BYTES};
use zksync_utils::bytecode::{hash_bytecode, validate_bytecode, InvalidBytecodeError};
use zksync_utils::{concat_and_hash, u256_to_h256};

// Local uses
use super::{EIP_1559_TX_TYPE, EIP_2930_TX_TYPE, EIP_712_TX_TYPE};
use crate::{
    fee::Fee,
    l1::L1Tx,
    l2::{L2Tx, TransactionType},
    web3::{signing::keccak256, types::AccessList},
    Address, Bytes, EIP712TypedStructure, Eip712Domain, L1TxCommonData, L2ChainId, Nonce,
    PackedEthSignature, StructBuilder, LEGACY_TX_TYPE, U256, U64,
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
    /// Nonce
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<U256>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub transaction_type: Option<U64>,
    /// Access list
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_list: Option<AccessList>,
    /// Eip712 meta
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eip712_meta: Option<Eip712Meta>,
}

impl CallRequest {
    /// Function to return a builder for a Call Request
    pub fn builder() -> CallRequestBuilder {
        CallRequestBuilder::default()
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
    pub fn to(mut self, to: Address) -> Self {
        self.call_request.to = Some(to);
        self
    }

    /// Set supplied gas (None for sensible default)
    pub fn gas(mut self, gas: U256) -> Self {
        self.call_request.gas = Some(gas);
        self
    }

    /// Set transfered value (None for no transfer)
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

    /// Set transfered value (None for no transfer)
    pub fn value(mut self, value: U256) -> Self {
        self.call_request.value = Some(value);
        self
    }

    /// Set data (None for empty data)
    pub fn data(mut self, data: Bytes) -> Self {
        self.call_request.data = Some(data);
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

#[derive(Debug, Error, PartialEq)]
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
    /// TooHighGas is a sanity error to avoid extremely big numbers specified
    /// to gas and pubdata price.
    #[error("{0}")]
    TooHighGas(String),
    /// OversizedData is returned if the raw tx size is greater
    /// than some meaningful limit a user might use. This is not a consensus error
    /// making the transaction invalid, rather a DOS protection.
    #[error("oversized data. max: {0}; actual: {0}")]
    OversizedData(usize, usize),
    #[error("gas per pub data limit is zero")]
    GasPerPubDataLimitZero,
}

/// Description of a Transaction, pending or in the chain.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRequest {
    /// Nonce
    pub nonce: U256,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_priority_fee_per_gas: Option<U256>,
    /// Input data
    pub input: Bytes,
    /// ECDSA recovery id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v: Option<U64>,
    /// ECDSA signature r, 32 bytes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r: Option<U256>,
    /// ECDSA signature s, 32 bytes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<U256>,
    /// Raw transaction data
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Bytes>,
    /// Transaction type, Some(1) for AccessList transaction, None for Legacy
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub transaction_type: Option<U64>,
    /// Access list
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_list: Option<AccessList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eip712_meta: Option<Eip712Meta>,
    /// Chain ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    pub factory_deps: Option<Vec<Vec<u8>>>,
    pub custom_signature: Option<Vec<u8>>,
    pub paymaster_params: Option<PaymasterParams>,
}

impl Eip712Meta {
    pub fn rlp_append(&self, rlp: &mut RlpStream) {
        rlp.append(&self.gas_per_pubdata);
        if let Some(factory_deps) = &self.factory_deps {
            rlp.begin_list(factory_deps.len());
            for dep in factory_deps.iter() {
                rlp.append(&dep.as_slice());
            }
        } else {
            rlp.begin_list(0);
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
            .map(|dep| hash_bytecode(&dep))
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
        self.eip712_meta
            .as_ref()
            .and_then(|meta| meta.custom_signature.as_ref())
            .cloned()
    }

    pub fn get_paymaster(&self) -> Option<Address> {
        self.eip712_meta
            .clone()
            .and_then(|meta| meta.paymaster_params)
            .map(|params| params.paymaster)
    }

    pub fn get_paymaster_input(&self) -> Option<Vec<u8>> {
        self.eip712_meta
            .clone()
            .and_then(|meta| meta.paymaster_params)
            .map(|params| params.paymaster_input)
    }

    pub fn get_factory_deps(&self) -> Vec<Vec<u8>> {
        self.eip712_meta
            .clone()
            .and_then(|meta| meta.factory_deps)
            .unwrap_or_default()
    }

    // returns packed eth signature if it is present
    fn get_packed_signature(&self) -> Result<PackedEthSignature, SerializationTransactionError> {
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

    pub fn get_signed_bytes(&self, signature: &PackedEthSignature, chain_id: L2ChainId) -> Vec<u8> {
        let mut rlp = RlpStream::new();
        self.rlp(&mut rlp, chain_id.as_u64(), Some(signature));
        let mut data = rlp.out().to_vec();
        if let Some(tx_type) = self.transaction_type {
            data.insert(0, tx_type.as_u64() as u8);
        }
        data
    }

    pub fn is_legacy_tx(&self) -> bool {
        self.transaction_type.is_none() || self.transaction_type == Some(LEGACY_TX_TYPE.into())
    }

    pub fn rlp(&self, rlp: &mut RlpStream, chain_id: u64, signature: Option<&PackedEthSignature>) {
        rlp.begin_unbounded_list();

        match self.transaction_type {
            // EIP-2930 (0x01)
            Some(x) if x == EIP_2930_TX_TYPE.into() => {
                // rlp_opt(rlp, &self.chain_id);
                rlp.append(&chain_id);
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
                // rlp_opt(rlp, &self.chain_id);
                rlp.append(&chain_id);
                rlp.append(&self.nonce);
                rlp_opt(rlp, &self.max_priority_fee_per_gas);
                rlp.append(&self.gas_price);
                rlp.append(&self.gas);
                rlp_opt(rlp, &self.to);
                rlp.append(&self.value);
                rlp.append(&self.input.0);
                access_list_rlp(rlp, &self.access_list);
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

        if let Some(signature) = signature {
            if self.is_legacy_tx() && chain_id != 0 {
                rlp.append(&signature.v_with_chain_id(chain_id));
            } else {
                rlp.append(&signature.v());
            }
            rlp.append(&U256::from_big_endian(signature.r()));
            rlp.append(&U256::from_big_endian(signature.s()));
        } else if self.is_legacy_tx() && chain_id != 0 {
            rlp.append(&chain_id);
            rlp.append(&0u8);
            rlp.append(&0u8);
        }

        if self.is_eip712_tx() {
            rlp.append(&chain_id);
            rlp_opt(rlp, &self.from);
            if let Some(meta) = &self.eip712_meta {
                meta.rlp_append(rlp);
            }
        }

        rlp.finalize_unbounded_list();
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

    pub fn from_bytes(
        bytes: &[u8],
        chain_id: L2ChainId,
    ) -> Result<(Self, H256), SerializationTransactionError> {
        let rlp;
        let mut tx = match bytes.first() {
            Some(x) if *x >= 0x80 => {
                rlp = Rlp::new(bytes);
                if rlp.item_count()? != 9 {
                    return Err(SerializationTransactionError::DecodeRlpError(
                        DecoderError::RlpIncorrectListLen,
                    ));
                }
                let v = rlp.val_at(6)?;
                let (_, tx_chain_id) = PackedEthSignature::unpack_v(v)
                    .map_err(|_| SerializationTransactionError::MalformedSignature)?;
                if tx_chain_id.is_some() && tx_chain_id != Some(chain_id.as_u64()) {
                    return Err(SerializationTransactionError::WrongChainId(tx_chain_id));
                }
                Self {
                    chain_id: tx_chain_id,
                    v: Some(rlp.val_at(6)?),
                    r: Some(rlp.val_at(7)?),
                    s: Some(rlp.val_at(8)?),
                    ..Self::decode_standard_fields(&rlp, 0)?
                }
            }
            Some(&EIP_1559_TX_TYPE) => {
                rlp = Rlp::new(&bytes[1..]);
                if rlp.item_count()? != 12 {
                    return Err(SerializationTransactionError::DecodeRlpError(
                        DecoderError::RlpIncorrectListLen,
                    ));
                }
                if let Ok(access_list_rlp) = rlp.at(8) {
                    if access_list_rlp.item_count()? > 0 {
                        return Err(SerializationTransactionError::AccessListsNotSupported);
                    }
                }

                let tx_chain_id = rlp.val_at(0).ok();
                if tx_chain_id != Some(chain_id.as_u64()) {
                    return Err(SerializationTransactionError::WrongChainId(tx_chain_id));
                }
                Self {
                    chain_id: tx_chain_id,
                    v: Some(rlp.val_at(9)?),
                    r: Some(rlp.val_at(10)?),
                    s: Some(rlp.val_at(11)?),
                    raw: Some(Bytes(rlp.as_raw().to_vec())),
                    transaction_type: Some(EIP_1559_TX_TYPE.into()),
                    ..Self::decode_eip1559_fields(&rlp, 1)?
                }
            }
            Some(&EIP_712_TX_TYPE) => {
                rlp = Rlp::new(&bytes[1..]);
                if rlp.item_count()? != 16 {
                    return Err(SerializationTransactionError::DecodeRlpError(
                        DecoderError::RlpIncorrectListLen,
                    ));
                }
                let tx_chain_id = rlp.val_at(10).ok();
                if tx_chain_id.is_some() && tx_chain_id != Some(chain_id.as_u64()) {
                    return Err(SerializationTransactionError::WrongChainId(tx_chain_id));
                }

                Self {
                    v: Some(rlp.val_at(7)?),
                    r: Some(rlp.val_at(8)?),
                    s: Some(rlp.val_at(9)?),
                    eip712_meta: Some(Eip712Meta {
                        gas_per_pubdata: rlp.val_at(12)?,
                        factory_deps: rlp.list_at(13).ok(),
                        custom_signature: rlp.val_at(14).ok(),
                        paymaster_params: if let Ok(params) = rlp.list_at(15) {
                            PaymasterParams::from_vector(params)?
                        } else {
                            None
                        },
                    }),
                    chain_id: tx_chain_id,
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
        let factory_deps_ref = tx
            .eip712_meta
            .as_ref()
            .and_then(|m| m.factory_deps.as_ref());
        if let Some(deps) = factory_deps_ref {
            validate_factory_deps(deps)?;
        }
        tx.raw = Some(Bytes(bytes.to_vec()));

        let default_signed_message = tx.get_default_signed_message(tx.chain_id)?;

        tx.from = match tx.from {
            Some(_) => tx.from,
            None => tx.recover_default_signer(default_signed_message).ok(),
        };

        let hash = tx.get_tx_hash_with_signed_message(&default_signed_message, chain_id)?;

        Ok((tx, hash))
    }

    fn get_default_signed_message(
        &self,
        chain_id: Option<u64>,
    ) -> Result<H256, SerializationTransactionError> {
        if self.is_eip712_tx() {
            let tx_chain_id =
                chain_id.ok_or(SerializationTransactionError::WrongChainId(chain_id))?;
            Ok(PackedEthSignature::typed_data_to_signed_bytes(
                &Eip712Domain::new(L2ChainId::try_from(tx_chain_id).unwrap()),
                self,
            ))
        } else {
            let mut rlp_stream = RlpStream::new();
            self.rlp(&mut rlp_stream, chain_id.unwrap_or_default(), None);
            let mut data = rlp_stream.out().to_vec();
            if let Some(tx_type) = self.transaction_type {
                data.insert(0, tx_type.as_u64() as u8);
            }
            Ok(PackedEthSignature::message_to_signed_bytes(&data))
        }
    }

    fn get_tx_hash_with_signed_message(
        &self,
        default_signed_message: &H256,
        chain_id: L2ChainId,
    ) -> Result<H256, SerializationTransactionError> {
        let hash = if self.is_eip712_tx() {
            concat_and_hash(
                *default_signed_message,
                H256(keccak256(&self.get_signature()?)),
            )
        } else if let Some(bytes) = &self.raw {
            H256(keccak256(&bytes.0))
        } else {
            let signature = self.get_packed_signature()?;
            H256(keccak256(&self.get_signed_bytes(&signature, chain_id)))
        };

        Ok(hash)
    }

    pub fn get_tx_hash(&self, chain_id: L2ChainId) -> Result<H256, SerializationTransactionError> {
        let default_signed_message = self.get_default_signed_message(Some(chain_id.as_u64()))?;
        self.get_tx_hash_with_signed_message(&default_signed_message, chain_id)
    }

    fn recover_default_signer(
        &self,
        default_signed_message: H256,
    ) -> Result<Address, SerializationTransactionError> {
        let signature = self.get_signature()?;
        PackedEthSignature::deserialize_packed(&signature)
            .map_err(|_| SerializationTransactionError::MalformedSignature)?
            .signature_recover_signer(&default_signed_message)
            .map_err(|_| SerializationTransactionError::MalformedSignature)?;

        let address = PackedEthSignature::deserialize_packed(&signature)
            .map_err(|_| SerializationTransactionError::MalformedSignature)?
            .signature_recover_signer(&default_signed_message)
            .map_err(|_| SerializationTransactionError::MalformedSignature)?;

        Ok(address)
    }

    fn get_fee_data_checked(&self) -> Result<Fee, SerializationTransactionError> {
        if self.gas_price > u64::MAX.into() {
            return Err(SerializationTransactionError::TooHighGas(
                "max fee per gas higher than 2^64-1".to_string(),
            ));
        }

        let gas_per_pubdata_limit = if let Some(meta) = &self.eip712_meta {
            if meta.gas_per_pubdata > u64::MAX.into() {
                return Err(SerializationTransactionError::TooHighGas(
                    "max fee per pubdata byte higher than 2^64-1".to_string(),
                ));
            } else if meta.gas_per_pubdata == U256::zero() {
                return Err(SerializationTransactionError::GasPerPubDataLimitZero);
            }
            meta.gas_per_pubdata
        } else {
            // For transactions that don't support corresponding field, a default is chosen.
            U256::from(MAX_GAS_PER_PUBDATA_BYTE)
        };

        let max_priority_fee_per_gas = self.max_priority_fee_per_gas.unwrap_or(self.gas_price);
        if max_priority_fee_per_gas > u64::MAX.into() {
            return Err(SerializationTransactionError::TooHighGas(
                "max priority fee per gas higher than 2^64-1".to_string(),
            ));
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
    pub fn from_request(
        value: TransactionRequest,
        max_tx_size: usize,
    ) -> Result<Self, SerializationTransactionError> {
        let fee = value.get_fee_data_checked()?;
        let nonce = value.get_nonce_checked()?;

        let raw_signature = value.get_signature().unwrap_or_default();
        // Destruct `eip712_meta` in one go to avoid cloning.
        let (factory_deps, paymaster_params) = value
            .eip712_meta
            .map(|eip712_meta| (eip712_meta.factory_deps, eip712_meta.paymaster_params))
            .unwrap_or_default();

        if let Some(deps) = factory_deps.as_ref() {
            validate_factory_deps(deps)?;
        }

        let mut tx = L2Tx::new(
            value
                .to
                .ok_or(SerializationTransactionError::ToAddressIsNull)?,
            value.input.0.clone(),
            nonce,
            fee,
            value.from.unwrap_or_default(),
            value.value,
            factory_deps,
            paymaster_params.unwrap_or_default(),
        );

        tx.common_data.transaction_type = match value.transaction_type.map(|t| t.as_u64() as u8) {
            Some(EIP_712_TX_TYPE) => TransactionType::EIP712Transaction,
            Some(EIP_1559_TX_TYPE) => TransactionType::EIP1559Transaction,
            Some(EIP_2930_TX_TYPE) => TransactionType::EIP2930Transaction,
            _ => TransactionType::LegacyTransaction,
        };
        // For fee calculation we use the same structure, as a result, signature may not be provided
        tx.set_raw_signature(raw_signature);

        if let Some(raw_bytes) = value.raw {
            tx.set_raw_bytes(raw_bytes);
        }
        tx.check_encoded_size(max_tx_size)?;
        Ok(tx)
    }

    /// Ensures that encoded transaction size is not greater than `max_tx_size`.
    fn check_encoded_size(&self, max_tx_size: usize) -> Result<(), SerializationTransactionError> {
        // since abi_encoding_len returns 32-byte words multiplication on 32 is needed
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
            factory_deps: None,
            custom_signature: Some(tx.common_data.signature.clone()),
            paymaster_params: Some(tx.common_data.paymaster_params.clone()),
        };
        meta.factory_deps = tx.execute.factory_deps.clone();
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
            input: call_request.data.unwrap_or_default(),
            transaction_type: call_request.transaction_type,
            access_list: call_request.access_list,
            eip712_meta: call_request.eip712_meta,
            ..Default::default()
        }
    }
}

impl TryFrom<CallRequest> for L1Tx {
    type Error = SerializationTransactionError;
    fn try_from(tx: CallRequest) -> Result<Self, Self::Error> {
        // L1 transactions have no limitations on the transaction size.
        let tx: L2Tx = L2Tx::from_request(tx.into(), USED_BOOTLOADER_MEMORY_BYTES)?;

        // Note, that while the user has theoretically provided the fee for ETH on L1,
        // the payment to the operator as well as refunds happen on L2 and so all the ETH
        // that the transaction requires to pay the operator needs to be minted on L2.
        let total_needed_eth =
            tx.execute.value + tx.common_data.fee.max_fee_per_gas * tx.common_data.fee.gas_limit;

        // Note, that we do not set refund_recipient here, to keep it explicitly 0,
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
    use super::*;
    use crate::web3::{
        api::Namespace,
        transports::test::TestTransport,
        types::{TransactionParameters, H256, U256},
    };
    use secp256k1::SecretKey;

    #[tokio::test]
    async fn decode_real_tx() {
        let accounts = crate::web3::api::Accounts::new(TestTransport::default());

        let pk = hex::decode("4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318")
            .unwrap();
        let address = PackedEthSignature::address_from_private_key(&H256::from_slice(&pk)).unwrap();
        let key = SecretKey::from_slice(&pk).unwrap();

        let tx = TransactionParameters {
            nonce: Some(U256::from(1u32)),
            to: Some(Address::random()),
            gas: Default::default(),
            gas_price: Some(U256::from(2u32)),
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            value: Default::default(),
            data: Bytes(vec![1, 2, 3]),
            chain_id: Some(270),
            transaction_type: None,
            access_list: None,
        };
        let signed_tx = accounts.sign_transaction(tx.clone(), &key).await.unwrap();
        let (tx2, _) = TransactionRequest::from_bytes(
            signed_tx.raw_transaction.0.as_slice(),
            L2ChainId::from(270),
        )
        .unwrap();
        assert_eq!(tx.gas, tx2.gas);
        assert_eq!(tx.gas_price.unwrap(), tx2.gas_price);
        assert_eq!(tx.nonce.unwrap(), tx2.nonce);
        assert_eq!(tx.data, tx2.input);
        assert_eq!(tx.value, tx2.value);
        assert_eq!(address, tx2.from.unwrap());
    }

    #[test]
    fn decode_rlp() {
        let private_key = H256::random();
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();

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
        tx.rlp(&mut rlp, 270, None);
        let data = rlp.out().to_vec();
        let msg = PackedEthSignature::message_to_signed_bytes(&data);
        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();
        tx.raw = Some(Bytes(data));
        let mut rlp = RlpStream::new();
        tx.rlp(&mut rlp, 270, Some(&signature));
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
        let private_key = H256::random();
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();

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
                factory_deps: Some(vec![vec![2; 32]]),
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
        tx.rlp(&mut rlp, 270, Some(&signature));
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
        let private_key = H256::random();
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();

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
                factory_deps: Some(vec![vec![2; 32]]),
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

        let encoded_tx = transaction_request.get_signed_bytes(&signature, L2ChainId::from(270));

        let (decoded_tx, _) =
            TransactionRequest::from_bytes(encoded_tx.as_slice(), L2ChainId::from(270)).unwrap();
        let recovered_signer = decoded_tx.from.unwrap();
        assert_eq!(address, recovered_signer);
    }

    #[test]
    fn check_recovered_public_key_eip712_with_wrong_chain_id() {
        let private_key = H256::random();
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();

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
                factory_deps: Some(vec![vec![2; 32]]),
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

        let encoded_tx = transaction_request.get_signed_bytes(&signature, L2ChainId::from(270));

        let decoded_tx =
            TransactionRequest::from_bytes(encoded_tx.as_slice(), L2ChainId::from(272));
        assert_eq!(
            decoded_tx,
            Err(SerializationTransactionError::WrongChainId(Some(270)))
        );
    }

    #[test]
    fn check_recovered_public_key_eip1559() {
        let private_key = H256::random();
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();

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
        transaction_request.rlp(&mut rlp_stream, 270, None);
        let mut data = rlp_stream.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);
        let msg = PackedEthSignature::message_to_signed_bytes(&data);

        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();
        transaction_request.raw = Some(Bytes(data));
        let mut rlp = RlpStream::new();
        transaction_request.rlp(&mut rlp, 270, Some(&signature));
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);

        let (decoded_tx, _) =
            TransactionRequest::from_bytes(data.as_slice(), L2ChainId::from(270)).unwrap();
        let recovered_signer = decoded_tx.from.unwrap();
        assert_eq!(address, recovered_signer);
    }

    #[test]
    fn check_recovered_public_key_eip1559_with_wrong_chain_id() {
        let private_key = H256::random();
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();

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
        transaction_request.rlp(&mut rlp_stream, 272, None);
        let mut data = rlp_stream.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);
        let msg = PackedEthSignature::message_to_signed_bytes(&data);

        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();
        transaction_request.raw = Some(Bytes(data));
        let mut rlp = RlpStream::new();
        transaction_request.rlp(&mut rlp, 272, Some(&signature));
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);

        let decoded_tx = TransactionRequest::from_bytes(data.as_slice(), L2ChainId::from(270));
        assert_eq!(
            decoded_tx,
            Err(SerializationTransactionError::WrongChainId(Some(272)))
        );
    }

    #[test]
    fn check_decode_eip1559_with_access_list() {
        let private_key = H256::random();
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();

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
        transaction_request.rlp(&mut rlp_stream, 270, None);
        let mut data = rlp_stream.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);
        let msg = PackedEthSignature::message_to_signed_bytes(&data);

        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();
        transaction_request.raw = Some(Bytes(data));
        let mut rlp = RlpStream::new();
        transaction_request.rlp(&mut rlp, 270, Some(&signature));
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_1559_TX_TYPE);

        let res = TransactionRequest::from_bytes(data.as_slice(), L2ChainId::from(270));
        assert_eq!(
            res,
            Err(SerializationTransactionError::AccessListsNotSupported)
        );
    }

    #[test]
    fn check_failed_to_decode_eip2930() {
        let private_key = H256::random();
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();

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
        transaction_request.rlp(&mut rlp_stream, 270, None);
        let mut data = rlp_stream.out().to_vec();
        data.insert(0, EIP_2930_TX_TYPE);
        let msg = PackedEthSignature::message_to_signed_bytes(&data);

        let signature = PackedEthSignature::sign_raw(&private_key, &msg).unwrap();
        transaction_request.raw = Some(Bytes(data));
        let mut rlp = RlpStream::new();
        transaction_request.rlp(&mut rlp, 270, Some(&signature));
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_2930_TX_TYPE);

        let res = TransactionRequest::from_bytes(data.as_slice(), L2ChainId::from(270));
        assert_eq!(
            res,
            Err(SerializationTransactionError::AccessListsNotSupported)
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
            L2Tx::from_request(tx1, usize::MAX);
        assert!(execute_tx1.is_ok());

        let tx2 = TransactionRequest {
            nonce: U256::from(u32::MAX as u64 + 1),
            to: Some(Address::repeat_byte(0x1)),
            from: Some(Address::repeat_byte(0x1)),
            value: U256::zero(),
            ..Default::default()
        };
        let execute_tx2: Result<L2Tx, SerializationTransactionError> =
            L2Tx::from_request(tx2, usize::MAX);
        assert_eq!(
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
            L2Tx::from_request(tx1, usize::MAX);
        assert_eq!(
            execute_tx1.unwrap_err(),
            SerializationTransactionError::TooHighGas(
                "max fee per gas higher than 2^64-1".to_string()
            )
        );

        let tx2 = TransactionRequest {
            to: Some(Address::repeat_byte(0x1)),
            from: Some(Address::repeat_byte(0x1)),
            value: U256::zero(),
            max_priority_fee_per_gas: Some(U256::MAX),
            ..Default::default()
        };
        let execute_tx2: Result<L2Tx, SerializationTransactionError> =
            L2Tx::from_request(tx2, usize::MAX);
        assert_eq!(
            execute_tx2.unwrap_err(),
            SerializationTransactionError::TooHighGas(
                "max priority fee per gas higher than 2^64-1".to_string()
            )
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
            L2Tx::from_request(tx3, usize::MAX);
        assert_eq!(
            execute_tx3.unwrap_err(),
            SerializationTransactionError::TooHighGas(
                "max fee per pubdata byte higher than 2^64-1".to_string()
            )
        );
    }

    #[test]
    fn transaction_request_with_oversize_data() {
        let random_tx_max_size = 1_000_000; // bytes
        let private_key = H256::random();
        let address = PackedEthSignature::address_from_private_key(&private_key).unwrap();
        // choose some number that devides on 8 and is > 1_000_000
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
                factory_deps: Some(factory_deps),
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
        tx.rlp(&mut rlp, 270, Some(&signature));
        let mut data = rlp.out().to_vec();
        data.insert(0, EIP_712_TX_TYPE);
        tx.raw = Some(Bytes(data.clone()));
        tx.v = Some(U64::from(signature.v()));
        tx.r = Some(U256::from_big_endian(signature.r()));
        tx.s = Some(U256::from_big_endian(signature.s()));
        let request =
            TransactionRequest::from_bytes(data.as_slice(), L2ChainId::from(270)).unwrap();
        assert!(matches!(
            L2Tx::from_request(request.0, random_tx_max_size),
            Err(SerializationTransactionError::OversizedData(_, _))
        ))
    }

    #[test]
    fn check_call_req_to_l2_tx_oversize_data() {
        let factory_dep = vec![2u8; 1600000];
        let random_tx_max_size = 100_000; // bytes
        let call_request = CallRequest {
            from: Some(Address::random()),
            to: Some(Address::random()),
            gas: Some(U256::from(12u32)),
            gas_price: Some(U256::from(12u32)),
            max_fee_per_gas: Some(U256::from(12u32)),
            max_priority_fee_per_gas: Some(U256::from(12u32)),
            value: Some(U256::from(12u32)),
            data: Some(Bytes(factory_dep)),
            nonce: None,
            transaction_type: Some(U64::from(EIP_712_TX_TYPE)),
            access_list: None,
            eip712_meta: None,
        };

        let try_to_l2_tx: Result<L2Tx, SerializationTransactionError> =
            L2Tx::from_request(call_request.into(), random_tx_max_size);

        assert!(matches!(
            try_to_l2_tx,
            Err(SerializationTransactionError::OversizedData(_, _))
        ));
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
            nonce: Some(U256::from(123u32)),
            transaction_type: Some(U64::from(EIP_712_TX_TYPE)),
            access_list: None,
            eip712_meta: None,
        };
        let l2_tx = L2Tx::from_request(
            call_request_with_nonce.clone().into(),
            USED_BOOTLOADER_MEMORY_BYTES,
        )
        .unwrap();
        assert_eq!(l2_tx.nonce(), Nonce(123u32));

        let mut call_request_without_nonce = call_request_with_nonce;
        call_request_without_nonce.nonce = None;

        let l2_tx = L2Tx::from_request(
            call_request_without_nonce.into(),
            USED_BOOTLOADER_MEMORY_BYTES,
        )
        .unwrap();
        assert_eq!(l2_tx.nonce(), Nonce(0u32));
    }
}
