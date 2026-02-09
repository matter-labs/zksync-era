//! Selected Web3 types copied from the `web3` crate.
//!
//! The majority of the code is copied verbatim from the `web3` crate 0.19.0, https://github.com/tomusdrw/rust-web3,
//! licensed under the MIT open-source license.

use std::fmt;

use ethabi::ethereum_types::{Address, H64};
use serde::{
    de::{Error, Unexpected, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};
use serde_json::Value;

use crate::{Bloom, H160, H256, U256, U64};

pub mod contract;
#[cfg(test)]
mod tests;

pub type Index = U64;

/// Number that can be either hex-encoded or decimal.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(untagged)]
pub enum U64Number {
    Hex(U64),
    Number(u64),
}

impl From<U64Number> for u64 {
    fn from(value: U64Number) -> Self {
        match value {
            U64Number::Hex(number) => number.as_u64(),
            U64Number::Number(number) => number,
        }
    }
}

impl From<u64> for U64Number {
    fn from(value: u64) -> Self {
        Self::Number(value)
    }
}

impl From<U64> for U64Number {
    fn from(value: U64) -> Self {
        Self::Hex(value)
    }
}

// `Signature`, `keccak256`: from `web3::signing`

/// A struct that represents the components of a secp256k1 signature.
#[derive(Debug)]
pub struct Signature {
    /// V component in Electrum format with chain-id replay protection.
    pub v: u64,
    /// R component of the signature.
    pub r: H256,
    /// S component of the signature.
    pub s: H256,
}

/// Compute the Keccak-256 hash of input bytes.
pub fn keccak256(bytes: &[u8]) -> [u8; 32] {
    use tiny_keccak::{Hasher, Keccak};

    let mut output = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(bytes);
    hasher.finalize(&mut output);
    output
}

// #[cfg(target_arch = "riscv32")]
// mod keccak_delegate {
//     use seq_macro::seq;

//     pub const KECCAK_SPECIAL5_STATE_AND_SCRATCH_U64_WORDS: usize = 31;

//     #[macro_export]
//     macro_rules! keccak_special5_load_initial_control {
//         () => {
//             core::arch::asm!(
//                 "add x10, x0, x0",
//                 out("x10") _,
//                 options(nostack, preserves_flags)
//             )
//         };
//     }

//     #[cfg(target_arch = "riscv32")]
//     #[macro_export]
//     macro_rules! keccak_special5_invoke {
//         ($state: expr) => {
//             core::arch::asm!(
//                 "csrrw x0, 0x7CB, x0",
//                 in("x11") $state,
//                 out("x10") _,
//                 options(nostack, preserves_flags)
//             )
//         };
//     }

//     pub(crate) fn keccak_f1600(state: &mut AlignedState) {
//         unsafe {
//             // start by setting initial control

//             let state_ptr = state.0.as_mut_ptr();
//             // start by setting initial control
//             keccak_special5_load_initial_control!();

//             // then run 24 rounds
//             seq!(round in 0..24 {
//                 // iota-theta-rho-chi-nopi: 5 iota_columnxor + 2 columnmix + 5 theta + 5 rho + 5*2 chi
//                 // control flow is guarded by circuit itself
//                 seq!(i in 0..27 {
//                     keccak_special5_invoke!(state_ptr);
//                 });
//             });

//             // then add +1 for the final iota
//             keccak_special5_invoke!(state_ptr);
//         }
//     }

//     #[cfg(not(target_endian = "little"))]
//     compile_error!("invalid arch - only intended for LE machines");

//     pub trait MiniDigest: Sized {
//         type HashOutput;

//         fn new() -> Self;
//         fn digest(input: impl AsRef<[u8]>) -> Self::HashOutput;
//         fn update(&mut self, input: impl AsRef<[u8]>);
//         fn finalize(self) -> Self::HashOutput;
//         fn finalize_reset(&mut self) -> Self::HashOutput;
//     }

//     // NB: repr(align(256)) ensures that the lowest u16 of the pointer can fully address
//     //     all the words without carry, s.t. we can very cheaply offset the ptr in-circuit
//     #[allow(dead_code)]
//     #[derive(Debug, Clone)]
//     #[repr(align(256))]
//     pub(crate) struct AlignedState([u64; KECCAK_SPECIAL5_STATE_AND_SCRATCH_U64_WORDS]);

//     // NOTE: Sha3 and Keccak differ only in padding, so we can make it generic for free,
//     // whether we will need it in practice or not. We also do not use a separate buffer for input,
//     // and instead XOR input directly into the state

//     const BUFFER_SIZE_U64_WORDS: usize = 17;
//     const BUFFER_SIZE_U32_WORDS: usize = BUFFER_SIZE_U64_WORDS * 2;
//     const BUFFER_SIZE_BYTES: usize = 17 * core::mem::size_of::<u64>();

//     #[allow(dead_code)]
//     #[derive(Debug, Clone)]
//     pub struct Keccak256Core<const SHA3: bool = false> {
//         state: AlignedState,
//         filled_bytes: usize,
//     }

//     #[allow(dead_code)]
//     pub type Keccak256 = Keccak256Core<false>;
//     #[allow(dead_code)]
//     pub type Sha3_256 = Keccak256Core<true>;

//     impl<const SHA3: bool> Keccak256Core<SHA3> {
//         #[inline(always)]
//         unsafe fn absorb_unaligned(&mut self, input: &mut &[u8]) {
//             let unalignment = self.filled_bytes % core::mem::size_of::<u32>();
//             if unalignment == 0 {
//                 return;
//             }
//             let to_absorb: usize =
//                 core::cmp::min(core::mem::size_of::<u32>() - unalignment, input.len());
//             let (slice_to_absorb, rest) = input.split_at_unchecked(to_absorb);
//             *input = rest;

//             let mut buffer = [0u8; core::mem::size_of::<u32>()];
//             let dst = buffer
//                 .get_unchecked_mut(unalignment..)
//                 .get_unchecked_mut(..to_absorb);
//             core::hint::assert_unchecked(slice_to_absorb.len() == dst.len());
//             dst.copy_from_slice(slice_to_absorb);

//             let u32_word_idx = self.filled_bytes / core::mem::size_of::<u32>();
//             let dst_word = self.state.0.as_mut_ptr().cast::<u32>().add(u32_word_idx);
//             dst_word.write(dst_word.read() ^ u32::from_le_bytes(buffer));

//             self.filled_bytes += to_absorb;
//         }

//         #[inline(always)]
//         unsafe fn absorb_aligned(&mut self, input: &mut &[u8]) {
//             if input.is_empty() {
//                 return;
//             }
//             debug_assert_eq!(self.filled_bytes % core::mem::size_of::<u32>(), 0);
//             debug_assert_ne!(self.filled_bytes, BUFFER_SIZE_BYTES);
//             debug_assert_eq!(
//                 (BUFFER_SIZE_BYTES - self.filled_bytes) % core::mem::size_of::<u32>(),
//                 0
//             );

//             let (u32_chunks, rest) = input.as_chunks::<4>();
//             *input = rest;
//             let max_words_to_absorb =
//                 (BUFFER_SIZE_BYTES - self.filled_bytes) / core::mem::size_of::<u32>();

//             let words_to_absorb = core::cmp::min(max_words_to_absorb, u32_chunks.len());
//             let u32_word_idx = self.filled_bytes / core::mem::size_of::<u32>();

//             let mut dst = self.state.0.as_mut_ptr().cast::<u32>().add(u32_word_idx);

//             let (fill_to_end_maybe, more) = u32_chunks.split_at_unchecked(words_to_absorb);
//             let mut it = fill_to_end_maybe.into_iter();
//             for _ in 0..words_to_absorb {
//                 dst.write(dst.read() ^ u32::from_le_bytes(*it.next().unwrap_unchecked()));
//                 dst = dst.add(1);
//             }
//             self.filled_bytes += words_to_absorb * core::mem::size_of::<u32>();
//             if self.filled_bytes == BUFFER_SIZE_BYTES {
//                 self.filled_bytes = 0;
//                 keccak_f1600(&mut self.state);
//             }

//             // then as many full fills as possible
//             let (full_buffer_fills, partial_fills) = more.as_chunks::<BUFFER_SIZE_U32_WORDS>();
//             for src in full_buffer_fills.into_iter() {
//                 debug_assert_eq!(self.filled_bytes, 0);
//                 let dst = self
//                     .state
//                     .0
//                     .as_mut_ptr()
//                     .cast::<[u32; BUFFER_SIZE_U32_WORDS]>()
//                     .as_mut()
//                     .unwrap_unchecked();
//                 core::hint::assert_unchecked(src.len() == dst.len());
//                 for (src, dst) in src.into_iter().zip(dst.iter_mut()) {
//                     *dst ^= u32::from_le_bytes(*src);
//                 }
//                 keccak_f1600(&mut self.state);
//             }

//             // and partial fill again
//             let words_to_absorb = partial_fills.len();
//             if words_to_absorb > 0 {
//                 debug_assert_eq!(self.filled_bytes, 0);
//             }
//             debug_assert!(words_to_absorb < BUFFER_SIZE_U32_WORDS);
//             let mut it = partial_fills.into_iter();
//             let mut dst = self.state.0.as_mut_ptr().cast::<u32>();
//             for _ in 0..words_to_absorb {
//                 dst.write(dst.read() ^ u32::from_le_bytes(*it.next().unwrap_unchecked()));
//                 dst = dst.add(1);
//             }
//             self.filled_bytes += words_to_absorb * core::mem::size_of::<u32>();
//             // can not trigger a permutation
//         }

//         #[inline(always)]
//         unsafe fn absorb_tail(&mut self, input: &[u8]) {
//             if input.is_empty() {
//                 return;
//             }
//             debug_assert!(input.len() < core::mem::size_of::<u32>());
//             debug_assert_eq!(self.filled_bytes % core::mem::size_of::<u32>(), 0);
//             let to_absorb = input.len();
//             let mut buffer = [0u8; core::mem::size_of::<u32>()];
//             buffer.get_unchecked_mut(..to_absorb).copy_from_slice(input);
//             let u32_word_idx = self.filled_bytes / core::mem::size_of::<u32>();
//             let dst = self.state.0.as_mut_ptr().cast::<u32>().add(u32_word_idx);
//             dst.write(dst.read() ^ u32::from_le_bytes(buffer));
//             self.filled_bytes += to_absorb;
//         }
//     }

//     impl<const SHA3: bool> MiniDigest for Keccak256Core<SHA3> {
//         type HashOutput = [u8; 32];

//         #[inline(always)]
//         fn new() -> Self {
//             Self {
//                 state: AlignedState([0; KECCAK_SPECIAL5_STATE_AND_SCRATCH_U64_WORDS]),
//                 filled_bytes: 0,
//             }
//         }

//         // #[inline(always)]
//         #[inline(never)]
//         fn update(&mut self, input: impl AsRef<[u8]>) {
//             let mut input = input.as_ref();

//             if input.len() == 0 {
//                 return;
//             }

//             // NOTE: reading unaligned u64/u32 to XOR bytes with the state is the same as copying it into aligned
//             // buffer first and then XORing anyway, so we will do it on the fly

//             unsafe {
//                 self.absorb_unaligned(&mut input);
//                 if self.filled_bytes == BUFFER_SIZE_BYTES {
//                     self.filled_bytes = 0;
//                     keccak_f1600(&mut self.state);
//                 }
//                 // absorb aligned will permut internellay if needed
//                 self.absorb_aligned(&mut input);

//                 // final absorb unaligned can not trigger permutation
//                 self.absorb_tail(input);

//                 debug_assert_ne!(self.filled_bytes, BUFFER_SIZE_BYTES);
//             };
//         }

//         #[inline(always)]
//         fn finalize(mut self) -> Self::HashOutput {
//             keccak_pad::<SHA3>(&mut self.state.0, self.filled_bytes);
//             keccak_f1600(&mut self.state);
//             unsafe { self.state.0.as_ptr().cast::<[u8; 32]>().read() }
//         }

//         #[inline(always)]
//         fn finalize_reset(&mut self) -> Self::HashOutput {
//             keccak_pad::<SHA3>(&mut self.state.0, self.filled_bytes);
//             keccak_f1600(&mut self.state);
//             let output = unsafe { self.state.0.as_ptr().cast::<[u8; 32]>().read() };
//             for dst in self.state.0.iter_mut() {
//                 *dst = 0;
//             }
//             self.filled_bytes = 0;

//             output
//         }

//         #[inline(always)]
//         fn digest(input: impl AsRef<[u8]>) -> Self::HashOutput {
//             let mut hasher = Self::new();
//             hasher.update(input);
//             hasher.finalize()
//         }
//     }

//     #[allow(dead_code)]
//     #[inline(always)]
//     fn keccak_pad<const SHA3: bool>(
//         state: &mut [u64; KECCAK_SPECIAL5_STATE_AND_SCRATCH_U64_WORDS],
//         len_filled_bytes: usize,
//     ) {
//         let pos_padding_start_u64 = len_filled_bytes / 8;
//         let padding_start = {
//             let len_leftover_bytes = len_filled_bytes % 8;
//             (if SHA3 { 0x06 } else { 0x01 }) << (len_leftover_bytes * 8)
//         };
//         state[pos_padding_start_u64] ^= padding_start;
//         state[16] ^= 0x80000000_00000000; // last bit is always there
//     }
// }

/// Hashes concatenation of the two provided hashes using `keccak256`.
pub fn keccak256_concat(hash1: H256, hash2: H256) -> H256 {
    let mut bytes = [0_u8; 64];
    bytes[..32].copy_from_slice(hash1.as_bytes());
    bytes[32..].copy_from_slice(hash2.as_bytes());

    // #[cfg(target_arch = "riscv32")]
    // {
    //     use keccak_delegate::{Keccak256, MiniDigest};

    //     let sha256 = Keccak256::digest(bytes);
    //     H256(sha256)
    // }

    // #[cfg(not(target_arch = "riscv32"))]
    // {
    H256(keccak256(&bytes))
    // }
}

// `Bytes`: from `web3::types::bytes`

/// Raw bytes wrapper
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct Bytes(pub Vec<u8>);

impl<T: Into<Vec<u8>>> From<T> for Bytes {
    fn from(data: T) -> Self {
        Bytes(data.into())
    }
}

impl Serialize for Bytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let mut serialized = "0x".to_owned();
            serialized.push_str(&hex::encode(&self.0));
            serializer.serialize_str(serialized.as_ref())
        } else {
            self.0.serialize(serializer)
        }
    }
}

impl<'a> Deserialize<'a> for Bytes {
    fn deserialize<D>(deserializer: D) -> Result<Bytes, D::Error>
    where
        D: Deserializer<'a>,
    {
        if deserializer.is_human_readable() {
            deserializer.deserialize_identifier(BytesVisitor)
        } else {
            Vec::<u8>::deserialize(deserializer).map(Bytes)
        }
    }
}

impl fmt::Debug for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let serialized = format!("0x{}", hex::encode(&self.0));
        f.debug_tuple("Bytes").field(&serialized).finish()
    }
}

struct BytesVisitor;

impl<'a> Visitor<'a> for BytesVisitor {
    type Value = Bytes;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a 0x-prefixed hex-encoded vector of bytes")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        if let Some(value) = value.strip_prefix("0x") {
            let bytes =
                hex::decode(value).map_err(|e| Error::custom(format!("Invalid hex: {}", e)))?;
            Ok(Bytes(bytes))
        } else {
            Err(Error::invalid_value(Unexpected::Str(value), &"0x prefix"))
        }
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: Error,
    {
        self.visit_str(value.as_ref())
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(Bytes(value.to_vec()))
    }

    fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(Bytes(value))
    }
}

// `Log`: from `web3::types::log`

/// Filter
#[derive(Default, Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// From Block
    #[serde(rename = "fromBlock", skip_serializing_if = "Option::is_none")]
    pub from_block: Option<BlockNumber>,
    /// To Block
    #[serde(rename = "toBlock", skip_serializing_if = "Option::is_none")]
    pub to_block: Option<BlockNumber>,
    /// Block Hash
    #[serde(rename = "blockHash", skip_serializing_if = "Option::is_none")]
    pub block_hash: Option<H256>,
    /// Address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<ValueOrArray<H160>>,
    /// Topics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<Option<ValueOrArray<H256>>>>,
    /// Limit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Default, Debug, PartialEq, Clone)]
pub struct ValueOrArray<T>(pub Vec<T>);

impl<T> ValueOrArray<T> {
    pub fn flatten(self) -> Vec<T> {
        self.0
    }
}

impl<T> From<T> for ValueOrArray<T> {
    fn from(value: T) -> Self {
        Self(vec![value])
    }
}

impl<T> Serialize for ValueOrArray<T>
where
    T: Serialize,
{
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

impl<'de, T> Deserialize<'de> for ValueOrArray<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr<T> {
            Single(T),
            Sequence(Vec<T>),
        }

        match Repr::<T>::deserialize(deserializer) {
            Ok(Repr::Single(element)) => Ok(Self(vec![element])),
            Ok(Repr::Sequence(elements)) => Ok(Self(elements)),
            Err(e) => Err(serde::de::Error::custom(format!(
                "Invalid parameter format. Expected either a single value or an array of values. \
                 Common issues: hex strings with extra spaces, malformed JSON objects, or invalid data types. \
                 Make sure hex strings are properly formatted (e.g., '0x123abc' not '0x123 abc'). \
                 Original error: {}",
                e
            ))),
        }
    }
}

// Filter Builder
#[derive(Default, Clone)]
pub struct FilterBuilder {
    filter: Filter,
}

impl FilterBuilder {
    /// Sets `from_block`. The fields `from_block` and `block_hash` are
    /// mutually exclusive. Setting `from_block` will clear a previously set
    /// `block_hash`.
    pub fn from_block(mut self, block: BlockNumber) -> Self {
        self.filter.block_hash = None;
        self.filter.from_block = Some(block);
        self
    }

    /// Sets `to_block`. The fields `to_block` and `block_hash` are mutually
    /// exclusive. Setting `to_block` will clear a previously set `block_hash`.
    pub fn to_block(mut self, block: BlockNumber) -> Self {
        self.filter.block_hash = None;
        self.filter.to_block = Some(block);
        self
    }

    /// Sets `block_hash`. The field `block_hash` and the pair `from_block` and
    /// `to_block` are mutually exclusive. Setting `block_hash` will clear a
    /// previously set `from_block` and `to_block`.
    pub fn block_hash(mut self, hash: H256) -> Self {
        self.filter.from_block = None;
        self.filter.to_block = None;
        self.filter.block_hash = Some(hash);
        self
    }

    /// Single address
    pub fn address(mut self, address: Vec<H160>) -> Self {
        self.filter.address = Some(ValueOrArray(address));
        self
    }

    /// Topics
    pub fn topics(
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
    pub fn topic_filter(self, topic_filter: ethabi::TopicFilter) -> Self {
        self.topics(
            topic_to_option(topic_filter.topic0),
            topic_to_option(topic_filter.topic1),
            topic_to_option(topic_filter.topic2),
            topic_to_option(topic_filter.topic3),
        )
    }

    /// Limit the result
    pub fn limit(mut self, limit: usize) -> Self {
        self.filter.limit = Some(limit);
        self
    }

    /// Returns filter
    pub fn build(&self) -> Filter {
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

/// A log produced by a transaction.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Log {
    /// H160
    pub address: H160,
    /// Topics
    pub topics: Vec<H256>,
    /// Data
    pub data: Bytes,
    /// Block Hash
    #[serde(rename = "blockHash")]
    pub block_hash: Option<H256>,
    /// Block Number
    #[serde(rename = "blockNumber")]
    pub block_number: Option<U64>,
    /// Transaction Hash
    #[serde(rename = "transactionHash")]
    pub transaction_hash: Option<H256>,
    /// Transaction Index
    #[serde(rename = "transactionIndex")]
    pub transaction_index: Option<Index>,
    /// Log Index in Block
    #[serde(rename = "logIndex")]
    pub log_index: Option<U256>,
    /// Log Index in Transaction
    #[serde(rename = "transactionLogIndex")]
    pub transaction_log_index: Option<U256>,
    /// Log Type
    #[serde(rename = "logType")]
    pub log_type: Option<String>,
    /// Removed
    pub removed: Option<bool>,
    /// L2 block timestamp
    #[serde(rename = "blockTimestamp")]
    pub block_timestamp: Option<U64>,
}

impl Log {
    /// Returns true if the log has been removed.
    pub fn is_removed(&self) -> bool {
        if let Some(val_removed) = self.removed {
            return val_removed;
        }
        if let Some(val_log_type) = &self.log_type {
            if val_log_type == "removed" {
                return true;
            }
        }
        false
    }
}

impl From<Log> for ethabi::RawLog {
    fn from(log: Log) -> Self {
        ethabi::RawLog {
            topics: log.topics,
            data: log.data.0,
        }
    }
}

// `BlockHeader`, `BlockId`, `BlockNumber`: from `web3::types::block`

/// The block header type returned from RPC calls.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BlockHeader {
    /// Hash of the block
    pub hash: Option<H256>,
    /// Hash of the parent
    #[serde(rename = "parentHash")]
    pub parent_hash: H256,
    /// Hash of the uncles
    #[serde(rename = "sha3Uncles")]
    #[serde(default)]
    pub uncles_hash: H256,
    /// Miner / author's address.
    #[serde(rename = "miner", default, deserialize_with = "null_to_default")]
    pub author: H160,
    /// State root hash
    #[serde(rename = "stateRoot")]
    pub state_root: H256,
    /// Transactions root hash
    #[serde(rename = "transactionsRoot")]
    pub transactions_root: H256,
    /// Transactions receipts root hash
    #[serde(rename = "receiptsRoot")]
    pub receipts_root: H256,
    /// Block number. None if pending.
    pub number: Option<U64>,
    /// Gas Used
    #[serde(rename = "gasUsed")]
    pub gas_used: U256,
    /// Gas Limit
    #[serde(rename = "gasLimit", default)]
    pub gas_limit: U256,
    /// Base fee per unit of gas (if past London)
    #[serde(rename = "baseFeePerGas", skip_serializing_if = "Option::is_none")]
    pub base_fee_per_gas: Option<U256>,
    /// Extra data
    #[serde(rename = "extraData")]
    pub extra_data: Bytes,
    /// Logs bloom
    #[serde(rename = "logsBloom")]
    pub logs_bloom: Bloom,
    /// Timestamp
    pub timestamp: U256,
    /// Difficulty
    #[serde(default)]
    pub difficulty: U256,
    /// Mix Hash
    #[serde(rename = "mixHash")]
    pub mix_hash: Option<H256>,
    /// Nonce
    pub nonce: Option<H64>,
}

/// The block type returned from RPC calls.
#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct Block<TX> {
    /// Hash of the block
    pub hash: Option<H256>,
    /// Hash of the parent
    #[serde(rename = "parentHash")]
    pub parent_hash: H256,
    /// Hash of the uncles
    #[serde(rename = "sha3Uncles", default)]
    pub uncles_hash: H256,
    /// Author's address.
    #[serde(rename = "miner", default, deserialize_with = "null_to_default")]
    pub author: H160,
    /// State root hash
    #[serde(rename = "stateRoot")]
    pub state_root: H256,
    /// Transactions root hash
    #[serde(rename = "transactionsRoot")]
    pub transactions_root: H256,
    /// Transactions receipts root hash
    #[serde(rename = "receiptsRoot")]
    pub receipts_root: H256,
    /// Block number. None if pending.
    pub number: Option<U64>,
    /// Gas Used
    #[serde(rename = "gasUsed")]
    pub gas_used: U256,
    /// Gas Limit
    #[serde(rename = "gasLimit", default)]
    pub gas_limit: U256,
    /// Base fee per unit of gas (if past London)
    #[serde(rename = "baseFeePerGas", skip_serializing_if = "Option::is_none")]
    pub base_fee_per_gas: Option<U256>,
    /// Extra data
    #[serde(rename = "extraData")]
    pub extra_data: Bytes,
    /// Logs bloom
    #[serde(rename = "logsBloom")]
    pub logs_bloom: Option<Bloom>,
    /// Timestamp
    pub timestamp: U256,
    /// Difficulty
    #[serde(default)]
    pub difficulty: U256,
    /// Total difficulty
    #[serde(rename = "totalDifficulty")]
    pub total_difficulty: Option<U256>,
    /// Seal fields
    #[serde(default, rename = "sealFields")]
    pub seal_fields: Vec<Bytes>,
    /// Uncles' hashes
    #[serde(default)]
    pub uncles: Vec<H256>,
    /// Transactions
    pub transactions: Vec<TX>,
    /// Size in bytes
    pub size: Option<U256>,
    /// Mix Hash
    #[serde(rename = "mixHash")]
    pub mix_hash: Option<H256>,
    /// Nonce
    pub nonce: Option<H64>,
    /// Excess blob gas
    #[serde(rename = "excessBlobGas")]
    pub excess_blob_gas: Option<U64>,
}

fn null_to_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    let option = Option::deserialize(deserializer)?;
    Ok(option.unwrap_or_default())
}

/// Block Identifier
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockId {
    /// By Hash
    Hash(H256),
    /// By Number
    Number(BlockNumber),
}

impl Serialize for BlockId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            BlockId::Hash(x) => {
                let mut s = serializer.serialize_struct("BlockIdEip1898", 1)?;
                s.serialize_field("blockHash", &format!("{:?}", x))?;
                s.end()
            }
            BlockId::Number(ref num) => num.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for BlockId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum BlockIdRepresentation {
            Number(BlockNumber),
            Hash {
                #[serde(rename = "blockHash")]
                block_hash: H256,
            },
        }

        Ok(match BlockIdRepresentation::deserialize(deserializer)? {
            BlockIdRepresentation::Number(number) => Self::Number(number),
            BlockIdRepresentation::Hash { block_hash } => Self::Hash(block_hash),
        })
    }
}

impl From<U64> for BlockId {
    fn from(num: U64) -> Self {
        BlockNumber::Number(num).into()
    }
}

impl From<BlockNumber> for BlockId {
    fn from(num: BlockNumber) -> Self {
        BlockId::Number(num)
    }
}

impl From<H256> for BlockId {
    fn from(hash: H256) -> Self {
        BlockId::Hash(hash)
    }
}

/// Block Number
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockNumber {
    /// Finalized block
    Finalized,
    /// Safe block
    Safe,
    /// Latest block
    Latest,
    /// Earliest block (genesis)
    Earliest,
    /// Pending block (not yet part of the blockchain)
    Pending,
    /// Block by number from canon chain
    Number(U64),
}

impl<T: Into<U64>> From<T> for BlockNumber {
    fn from(num: T) -> Self {
        BlockNumber::Number(num.into())
    }
}

impl Serialize for BlockNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *self {
            BlockNumber::Number(ref x) => serializer.serialize_str(&format!("0x{:x}", x)),
            BlockNumber::Latest => serializer.serialize_str("latest"),
            BlockNumber::Earliest => serializer.serialize_str("earliest"),
            BlockNumber::Pending => serializer.serialize_str("pending"),
            BlockNumber::Finalized => serializer.serialize_str("finalized"),
            BlockNumber::Safe => serializer.serialize_str("safe"),
        }
    }
}

impl<'a> Deserialize<'a> for BlockNumber {
    fn deserialize<D>(deserializer: D) -> Result<BlockNumber, D::Error>
    where
        D: Deserializer<'a>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "latest" => Ok(BlockNumber::Latest),
            "earliest" => Ok(BlockNumber::Earliest),
            "pending" => Ok(BlockNumber::Pending),
            "finalized" => Ok(BlockNumber::Finalized),
            "safe" => Ok(BlockNumber::Safe),
            _ if value.starts_with("0x") => U64::from_str_radix(&value[2..], 16)
                .map(BlockNumber::Number)
                .map_err(|e| D::Error::custom(format!("invalid block number: {}", e))),
            _ => Err(D::Error::custom(
                "invalid block number: missing 0x prefix".to_string(),
            )),
        }
    }
}

// `AccessList`, `AccessListItem`, `TransactionReceipt`, `SignedTransaction`: from `web3::types::transaction`

/// Access list
pub type AccessList = Vec<AccessListItem>;

/// Access list item
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessListItem {
    /// Accessed address
    pub address: Address,
    /// Accessed storage keys
    pub storage_keys: Vec<H256>,
}

/// Description of a Transaction, pending or in the chain.
#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct Transaction {
    /// Hash
    pub hash: H256,
    /// Nonce
    pub nonce: U256,
    /// Block hash. None when pending.
    #[serde(rename = "blockHash")]
    pub block_hash: Option<H256>,
    /// Block number. None when pending.
    #[serde(rename = "blockNumber")]
    pub block_number: Option<U64>,
    /// Transaction Index. None when pending.
    #[serde(rename = "transactionIndex")]
    pub transaction_index: Option<Index>,
    /// Sender
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    /// Recipient (None when contract creation)
    pub to: Option<Address>,
    /// Transferred value
    pub value: U256,
    /// Gas Price
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<U256>,
    /// Gas amount
    pub gas: U256,
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
    #[serde(
        rename = "accessList",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub access_list: Option<AccessList>,
    /// Max fee per gas
    #[serde(rename = "maxFeePerGas", skip_serializing_if = "Option::is_none")]
    pub max_fee_per_gas: Option<U256>,
    /// miner bribe
    #[serde(
        rename = "maxPriorityFeePerGas",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_priority_fee_per_gas: Option<U256>,
}

/// "Receipt" of an executed transaction: details of its execution.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionReceipt {
    /// Transaction hash.
    #[serde(rename = "transactionHash")]
    pub transaction_hash: H256,
    /// Index within the block.
    #[serde(rename = "transactionIndex")]
    pub transaction_index: Index,
    /// Hash of the block this transaction was included within.
    #[serde(rename = "blockHash")]
    pub block_hash: Option<H256>,
    /// Number of the block this transaction was included within.
    #[serde(rename = "blockNumber")]
    pub block_number: Option<U64>,
    /// Sender
    /// Note: default address if the client did not return this value
    /// (maintains backwards compatibility for <= 0.7.0 when this field was missing)
    #[serde(default)]
    pub from: Address,
    /// Recipient (None when contract creation)
    /// Note: Also `None` if the client did not return this value
    /// (maintains backwards compatibility for <= 0.7.0 when this field was missing)
    #[serde(default)]
    pub to: Option<Address>,
    /// Cumulative gas used within the block after this was executed.
    #[serde(rename = "cumulativeGasUsed")]
    pub cumulative_gas_used: U256,
    /// Gas used by this transaction alone.
    ///
    /// Gas used is `None` if the the client is running in light client mode.
    #[serde(rename = "gasUsed")]
    pub gas_used: Option<U256>,
    /// Contract address created, or `None` if not a deployment.
    #[serde(rename = "contractAddress")]
    pub contract_address: Option<Address>,
    /// Logs generated within this transaction.
    pub logs: Vec<Log>,
    /// Status: either 1 (success) or 0 (failure).
    pub status: Option<U64>,
    /// State root.
    pub root: Option<H256>,
    /// Logs bloom
    #[serde(rename = "logsBloom")]
    pub logs_bloom: Bloom,
    /// Transaction type, Some(1) for AccessList transaction, None for Legacy
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub transaction_type: Option<U64>,
    /// Effective gas price
    #[serde(rename = "effectiveGasPrice")]
    pub effective_gas_price: Option<U256>,
}

/// Data for offline signed transaction
#[derive(Clone, Debug, PartialEq)]
pub struct SignedTransaction {
    /// The given message hash
    pub message_hash: H256,
    /// V value with chain replay protection.
    pub v: u64,
    /// R value.
    pub r: H256,
    /// S value.
    pub s: H256,
    /// The raw signed transaction ready to be sent with `send_raw_transaction`
    pub raw_transaction: Bytes,
    /// The transaction hash for the RLP encoded transaction.
    pub transaction_hash: H256,
}

/// Transaction Identifier
#[derive(Clone, Debug, PartialEq)]
pub enum TransactionId {
    /// By hash
    Hash(H256),
    /// By block and index
    Block(BlockId, Index),
}

impl From<H256> for TransactionId {
    fn from(hash: H256) -> Self {
        TransactionId::Hash(hash)
    }
}

// `CallRequest`, `TransactionCondition`: from `web3::types::transaction_request`

/// Call contract request (eth_call / eth_estimateGas)
///
/// When using this for `eth_estimateGas`, all the fields
/// are optional. However, for usage in `eth_call` the
/// `to` field must be provided.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct CallRequest {
    /// Sender address (None for arbitrary address)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    /// To address (None allowed for eth_estimateGas)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Address>,
    /// Supplied gas (None for sensible default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas: Option<U256>,
    /// Gas price (None for sensible default)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<U256>,
    /// Transferred value (None for no transfer)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<U256>,
    /// Data (None for empty data)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Bytes>,
    /// Transaction type, Some(1) for AccessList transaction, None for Legacy
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub transaction_type: Option<U64>,
    /// Access list
    #[serde(
        rename = "accessList",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub access_list: Option<AccessList>,
    /// Max fee per gas
    #[serde(rename = "maxFeePerGas", skip_serializing_if = "Option::is_none")]
    pub max_fee_per_gas: Option<U256>,
    /// miner bribe
    #[serde(
        rename = "maxPriorityFeePerGas",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_priority_fee_per_gas: Option<U256>,
}

/// Represents condition on minimum block number or block timestamp.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TransactionCondition {
    /// Valid at this minimum block number.
    #[serde(rename = "block")]
    Block(u64),
    /// Valid at given Unix time.
    #[serde(rename = "time")]
    Timestamp(u64),
}

// `FeeHistory`: from `web3::types::fee_history`
// Adapted to support blobs.

// /// The fee history type returned from `eth_feeHistory` call.
// #[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
// #[serde(rename_all = "camelCase")]
// pub struct FeeHistory {
//     /// Lowest number block of the returned range.
//     pub oldest_block: BlockNumber,
//     /// A vector of block base fees per gas. This includes the next block after the newest of the returned range,
//     /// because this value can be derived from the newest block. Zeroes are returned for pre-EIP-1559 blocks.
//     #[serde(default)] // some node implementations skip empty lists
//     pub base_fee_per_gas: Vec<U256>,
//     /// A vector of block gas used ratios. These are calculated as the ratio of gas used and gas limit.
//     #[serde(default)] // some node implementations skip empty lists
//     pub gas_used_ratio: Vec<f64>,
//     /// A vector of effective priority fee per gas data points from a single block. All zeroes are returned if
//     /// the block is empty. Returned only if requested.
//     pub reward: Option<Vec<Vec<U256>>>,
//     /// An array of base fees per blob gas for blocks. This includes the next block following the newest in the
//     /// returned range, as this value can be derived from the latest block. For blocks before EIP-4844, zeroes
//     /// are returned.
//     #[serde(default)] // some node implementations skip empty lists
//     pub base_fee_per_blob_gas: Vec<U256>,
//     /// An array showing the ratios of blob gas used in blocks. These ratios are calculated by dividing blobGasUsed
//     /// by the maximum blob gas per block.
//     #[serde(default)] // some node implementations skip empty lists
//     pub blob_gas_used_ratio: Vec<f64>,
// }

// `SyncInfo`, `SyncState`: from `web3::types::sync_state`

/// Information about current blockchain syncing operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncInfo {
    /// The block at which import began.
    pub starting_block: U256,
    /// The highest currently synced block.
    pub current_block: U256,
    /// The estimated highest block.
    pub highest_block: U256,
}

/// The current state of blockchain syncing operations.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncState {
    /// Blockchain is syncing.
    Syncing(SyncInfo),
    /// Blockchain is not syncing.
    NotSyncing,
}

// Sync info from subscription has a different key format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SubscriptionSyncInfo {
    /// The block at which import began.
    pub starting_block: U256,
    /// The highest currently synced block.
    pub current_block: U256,
    /// The estimated highest block.
    pub highest_block: U256,
}

impl From<SubscriptionSyncInfo> for SyncInfo {
    fn from(s: SubscriptionSyncInfo) -> Self {
        Self {
            starting_block: s.starting_block,
            current_block: s.current_block,
            highest_block: s.highest_block,
        }
    }
}

// The `eth_syncing` method returns either `false` or an instance of the sync info object.
// This doesn't play particularly well with the features exposed by `serde_derive`,
// so we use the custom impls below to ensure proper behavior.
impl<'de> Deserialize<'de> for SyncState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct SubscriptionSyncState {
            pub syncing: bool,
            pub status: Option<SubscriptionSyncInfo>,
        }

        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(untagged)]
        enum SyncStateVariants {
            Rpc(SyncInfo),
            Subscription(SubscriptionSyncState),
            Boolean(bool),
        }

        let v: SyncStateVariants = Deserialize::deserialize(deserializer)?;
        match v {
            SyncStateVariants::Rpc(info) => Ok(SyncState::Syncing(info)),
            SyncStateVariants::Subscription(state) => match state.status {
                None if !state.syncing => Ok(SyncState::NotSyncing),
                Some(ref info) if state.syncing => Ok(SyncState::Syncing(info.clone().into())),
                _ => Err(D::Error::custom(
                    "expected object or `syncing = false`, got `syncing = true`",
                )),
            },
            SyncStateVariants::Boolean(boolean) => {
                if !boolean {
                    Ok(SyncState::NotSyncing)
                } else {
                    Err(D::Error::custom("expected object or `false`, got `true`"))
                }
            }
        }
    }
}

impl Serialize for SyncState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            SyncState::Syncing(info) => info.serialize(serializer),
            SyncState::NotSyncing => false.serialize(serializer),
        }
    }
}

// `TraceFilter`: from `web3::types::trace_filtering`

/// Trace filter
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TraceFilter {
    /// From block
    #[serde(rename = "fromBlock", skip_serializing_if = "Option::is_none")]
    from_block: Option<BlockNumber>,
    /// To block
    #[serde(rename = "toBlock", skip_serializing_if = "Option::is_none")]
    to_block: Option<BlockNumber>,
    /// From address
    #[serde(rename = "fromAddress", skip_serializing_if = "Option::is_none")]
    from_address: Option<Vec<Address>>,
    /// To address
    #[serde(rename = "toAddress", skip_serializing_if = "Option::is_none")]
    to_address: Option<Vec<Address>>,
    /// Output offset
    #[serde(skip_serializing_if = "Option::is_none")]
    after: Option<usize>,
    /// Output amount
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<usize>,
}

// `Work`: from `web3::types::work`

/// Miner's work package
#[derive(Debug, PartialEq, Eq)]
pub struct Work {
    /// The proof-of-work hash.
    pub pow_hash: H256,
    /// The seed hash.
    pub seed_hash: H256,
    /// The target.
    pub target: H256,
    /// The block number: this isn't always stored.
    pub number: Option<u64>,
}

impl<'a> Deserialize<'a> for Work {
    fn deserialize<D>(deserializer: D) -> Result<Work, D::Error>
    where
        D: Deserializer<'a>,
    {
        panic!("Not supported");
        // let v: Value = Deserialize::deserialize(deserializer)?;

        // let (pow_hash, seed_hash, target, number) =
        //     serde_json::from_value::<(H256, H256, H256, u64)>(v.clone())
        //         .map(|(pow_hash, seed_hash, target, number)| {
        //             (pow_hash, seed_hash, target, Some(number))
        //         })
        //         .or_else(|_| {
        //             serde_json::from_value::<(H256, H256, H256)>(v)
        //                 .map(|(pow_hash, seed_hash, target)| (pow_hash, seed_hash, target, None))
        //         })
        //         .map_err(|e| D::Error::custom(format!("Cannot deserialize Work: {:?}", e)))?;

        // Ok(Work {
        //     pow_hash,
        //     seed_hash,
        //     target,
        //     number,
        // })
    }
}

impl Serialize for Work {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.number.as_ref() {
            Some(num) => (
                &self.pow_hash,
                &self.seed_hash,
                &self.target,
                U256::from(*num),
            )
                .serialize(s),
            None => (&self.pow_hash, &self.seed_hash, &self.target).serialize(s),
        }
    }
}
