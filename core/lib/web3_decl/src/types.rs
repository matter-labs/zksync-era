//! Web3 API types definitions.
//!
//! Most of the types are re-exported from the `web3` crate, but some of them maybe extended with
//! new variants (enums) or optional fields (structures).
//!
//! These "extensions" are required to provide more ZKsync-specific information while remaining Web3-compilant.

use core::{
    convert::{TryFrom, TryInto},
    fmt,
    marker::PhantomData,
};

use rlp::Rlp;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
pub use zksync_types::{
    api::{Block, BlockNumber, Log, TransactionReceipt, TransactionRequest},
    ethabi,
    web3::{BlockHeader, Bytes, CallRequest, FeeHistory, Index, SyncState, TraceFilter, Work},
    Address, Transaction, H160, H256, H64, U256, U64,
};

/// Token in the ZKsync network
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub l1_address: Address,
    pub l2_address: Address,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

/// Helper structure used to parse deserialized `Ethereum` transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionCalldata {
    pub selector: [u8; 4],
    pub data: Vec<u8>,
}

/// Helper structure used to parse deserialized `Ethereum` transaction according to `EIP-2718`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EIP2718TransactionCallData(TransactionCalldata);

impl rlp::Decodable for TransactionCalldata {
    fn decode(d: &Rlp) -> Result<Self, rlp::DecoderError> {
        if d.item_count()? != 9 {
            return Err(rlp::DecoderError::RlpIncorrectListLen);
        }

        let calldata: Vec<u8> = d.val_at(5)?;

        Self::try_from(calldata).map_err(|_| rlp::DecoderError::RlpIncorrectListLen)
    }
}

impl rlp::Decodable for EIP2718TransactionCallData {
    fn decode(d: &Rlp) -> Result<Self, rlp::DecoderError> {
        if d.item_count()? != 12 {
            return Err(rlp::DecoderError::RlpIncorrectListLen);
        }

        let calldata: Vec<u8> = d.val_at(7)?;

        TransactionCalldata::try_from(calldata)
            .map(Self)
            .map_err(|_| rlp::DecoderError::RlpIncorrectListLen)
    }
}

impl From<EIP2718TransactionCallData> for TransactionCalldata {
    fn from(EIP2718TransactionCallData(calldata): EIP2718TransactionCallData) -> Self {
        calldata
    }
}

impl TryFrom<Vec<u8>> for TransactionCalldata {
    // TODO (SMA-1613): improve length error
    type Error = usize;

    fn try_from(mut calldata: Vec<u8>) -> Result<Self, Self::Error> {
        let selector = calldata
            .get(0..4)
            .ok_or(calldata.len())?
            .try_into()
            .unwrap();
        let data = calldata.split_off(4);

        Ok(TransactionCalldata { selector, data })
    }
}

// Changes watched by the given `Filter`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FilterChanges {
    Hashes(Vec<H256>),
    Logs(Vec<Log>),
    Empty([u8; 0]),
}

/// Either value or array of values.
///
/// A value must serialize into a string.
#[derive(Default, Debug, PartialEq, Clone)]
pub struct ValueOrArray<T>(pub Vec<T>);

impl<T> From<T> for ValueOrArray<T> {
    fn from(value: T) -> Self {
        Self(vec![value])
    }
}

impl<T: Serialize> Serialize for ValueOrArray<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0.len() {
            0 => serializer.serialize_none(),
            1 => Serialize::serialize(&self.0[0], serializer),
            _ => Serialize::serialize(&self.0, serializer),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for ValueOrArray<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> de::Visitor<'de> for Visitor<T> {
            type Value = ValueOrArray<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("string value or sequence of values")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                use serde::de::IntoDeserializer;

                Deserialize::deserialize(value.into_deserializer())
                    .map(|value| ValueOrArray(vec![value]))
            }

            fn visit_seq<S>(self, mut visitor: S) -> Result<Self::Value, S::Error>
            where
                S: de::SeqAccess<'de>,
            {
                let mut elements = Vec::with_capacity(visitor.size_hint().unwrap_or(1));
                while let Some(element) = visitor.next_element()? {
                    elements.push(element);
                }
                Ok(ValueOrArray(elements))
            }
        }

        deserializer.deserialize_any(Visitor(PhantomData))
    }
}

/// Filter
#[derive(Default, Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// From Block
    #[serde(rename = "fromBlock", skip_serializing_if = "Option::is_none")]
    pub from_block: Option<BlockNumber>,
    /// To Block
    #[serde(rename = "toBlock", skip_serializing_if = "Option::is_none")]
    pub to_block: Option<BlockNumber>,
    /// Address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<ValueOrArray<H160>>,
    /// Topics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<Option<ValueOrArray<H256>>>>,
    #[serde(rename = "blockHash", skip_serializing_if = "Option::is_none")]
    pub block_hash: Option<H256>,
}

/// Filter Builder
#[derive(Default, Clone)]
pub struct FilterBuilder {
    filter: Filter,
}

impl FilterBuilder {
    /// Sets from block
    pub fn set_from_block(mut self, block: BlockNumber) -> Self {
        self.filter.from_block = Some(block);
        self
    }

    /// Sets to block
    pub fn set_to_block(mut self, block: BlockNumber) -> Self {
        self.filter.to_block = Some(block);
        self
    }

    /// Single address
    pub fn set_address(mut self, address: Vec<H160>) -> Self {
        self.filter.address = Some(ValueOrArray(address));
        self
    }

    /// Topics
    pub fn set_topics(
        mut self,
        topic1: Option<Vec<H256>>,
        topic2: Option<Vec<H256>>,
        topic3: Option<Vec<H256>>,
        topic4: Option<Vec<H256>>,
    ) -> Self {
        let mut topics = vec![topic1, topic2, topic3, topic4]
            .into_iter()
            .rev()
            .skip_while(Option::is_none)
            .map(|option| option.map(ValueOrArray))
            .collect::<Vec<_>>();
        topics.reverse();

        self.filter.topics = Some(topics);
        self
    }

    /// Sets the topics according to the given `ethabi` topic filter
    pub fn set_topic_filter(self, topic_filter: ethabi::TopicFilter) -> Self {
        self.set_topics(
            topic_to_option(topic_filter.topic0),
            topic_to_option(topic_filter.topic1),
            topic_to_option(topic_filter.topic2),
            topic_to_option(topic_filter.topic3),
        )
    }

    /// Returns filter
    pub fn build(&self) -> Filter {
        self.filter.clone()
    }
}

#[derive(Default, Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct PubSubFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<ValueOrArray<H160>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<Option<ValueOrArray<H256>>>>,
}

impl PubSubFilter {
    pub fn matches(&self, log: &Log) -> bool {
        if let Some(addresses) = &self.address {
            if !addresses.0.contains(&log.address) {
                return false;
            }
        }
        if let Some(all_topics) = &self.topics {
            for (idx, expected_topics) in all_topics.iter().enumerate() {
                if let Some(expected_topics) = expected_topics {
                    if let Some(actual_topic) = log.topics.get(idx) {
                        if !expected_topics.0.contains(actual_topic) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[derive(Default, Clone)]
pub struct PubSubFilterBuilder {
    filter: PubSubFilter,
}

impl PubSubFilterBuilder {
    /// Single address
    pub fn set_address(mut self, address: Vec<H160>) -> Self {
        self.filter.address = Some(ValueOrArray(address));
        self
    }

    /// Topics
    pub fn set_topics(
        mut self,
        topic1: Option<Vec<H256>>,
        topic2: Option<Vec<H256>>,
        topic3: Option<Vec<H256>>,
        topic4: Option<Vec<H256>>,
    ) -> Self {
        let mut topics = vec![topic1, topic2, topic3, topic4]
            .into_iter()
            .rev()
            .skip_while(Option::is_none)
            .map(|option| option.map(ValueOrArray))
            .collect::<Vec<_>>();
        topics.reverse();

        self.filter.topics = Some(topics);
        self
    }

    /// Sets the topics according to the given `ethabi` topic filter
    pub fn set_topic_filter(self, topic_filter: ethabi::TopicFilter) -> Self {
        self.set_topics(
            topic_to_option(topic_filter.topic0),
            topic_to_option(topic_filter.topic1),
            topic_to_option(topic_filter.topic2),
            topic_to_option(topic_filter.topic3),
        )
    }

    /// Returns filter
    pub fn build(&self) -> PubSubFilter {
        self.filter.clone()
    }
}

/// Converts a `Topic` to an equivalent `Option<Vec<T>>`, suitable for `FilterBuilder::topics`
fn topic_to_option<T>(topic: ethabi::Topic<T>) -> Option<Vec<T>> {
    match topic {
        ethabi::Topic::Any => None,
        ethabi::Topic::OneOf(v) => Some(v),
        ethabi::Topic::This(t) => Some(vec![t]),
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PubSubResult {
    Header(BlockHeader),
    Log(Log),
    TxHash(H256),
    Syncing(bool),
}

#[cfg(test)]
mod tests {
    use zksync_types::api::{BlockId, BlockIdVariant};

    use super::*;

    #[test]
    fn get_block_number_serde() {
        let test_vector = &[
            (r#""committed""#, BlockNumber::Committed),
            (r#""finalized""#, BlockNumber::Finalized),
            (r#""pending""#, BlockNumber::Pending),
            (r#""latest""#, BlockNumber::Latest),
            (r#""earliest""#, BlockNumber::Earliest),
            (r#""0x1""#, BlockNumber::Number(1.into())),
            (r#""0x10""#, BlockNumber::Number(16.into())),
        ];

        for (serialized_repr, deserialized_repr) in test_vector {
            let serialized = serde_json::to_string(deserialized_repr).unwrap();
            assert_eq!(&serialized, serialized_repr);

            let deserialized: BlockNumber = serde_json::from_str(serialized_repr).unwrap();
            assert_eq!(&deserialized, deserialized_repr);
        }
    }

    #[test]
    fn get_block_id_serde() {
        let test_vector = &[
            (
                r#""0x0000000000000000000000000000000000000000000000000000000000000000""#,
                BlockId::Hash(H256::default()),
            ),
            (r#""latest""#, BlockId::Number(BlockNumber::Latest)),
            (r#""0x10""#, BlockId::Number(BlockNumber::Number(16.into()))),
        ];

        for (serialized_repr, deserialized_repr) in test_vector {
            let serialized = serde_json::to_string(deserialized_repr).unwrap();
            assert_eq!(&serialized, serialized_repr);

            let deserialized: BlockId = serde_json::from_str(serialized_repr).unwrap();
            assert_eq!(&deserialized, deserialized_repr);
        }
    }

    #[test]
    fn block_id_variant_serializing() {
        let test_vector = &[
            (r#""latest""#, BlockId::Number(BlockNumber::Latest)),
            (r#""0x10""#, BlockId::Number(BlockNumber::Number(16.into()))),
            (
                r#"{"blockHash": "0x0000000000000000000000000000000000000000000000000000000000000000"}"#,
                BlockId::Hash(H256::default()),
            ),
            (
                r#"{"blockNumber": "0x10"}"#,
                BlockId::Number(BlockNumber::Number(16.into())),
            ),
        ];

        for (serialized_repr, expected_block_id) in test_vector {
            let deserialized: BlockIdVariant = serde_json::from_str(serialized_repr).unwrap();
            let actual_block_id: BlockId = deserialized.into();
            assert_eq!(&actual_block_id, expected_block_id);
        }
    }

    #[test]
    fn serializing_value_or_array() {
        let value = ValueOrArray::from(Address::repeat_byte(0x1f));
        let json = serde_json::to_value(value.clone()).unwrap();
        assert_eq!(
            json,
            serde_json::json!("0x1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f")
        );

        let restored_value: ValueOrArray<Address> = serde_json::from_value(json).unwrap();
        assert_eq!(restored_value, value);

        let value = ValueOrArray(vec![Address::repeat_byte(0x1f), Address::repeat_byte(0x23)]);
        let json = serde_json::to_value(value.clone()).unwrap();
        assert_eq!(
            json,
            serde_json::json!([
                "0x1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f",
                "0x2323232323232323232323232323232323232323",
            ])
        );

        let restored_value: ValueOrArray<Address> = serde_json::from_value(json).unwrap();
        assert_eq!(restored_value, value);
    }
}
