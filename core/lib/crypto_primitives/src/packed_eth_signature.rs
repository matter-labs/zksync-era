use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use zksync_basic_types::{web3::keccak256, Address, H256};
use zksync_utils::ZeroPrefixHexSerde;

use crate::{
    ecdsa_signature::{
        public_to_address, recover, sign, Error as ParityCryptoError, K256PrivateKey,
        Signature as ETHSignature,
    },
    eip712_signature::typed_structure::{EIP712TypedStructure, Eip712Domain},
};

/// Struct used for working with Ethereum signatures created using eth_sign (using geth, ethers.js, etc)
/// message is serialized as 65 bytes long `0x` prefixed string.
///
/// Some notes on implementation of methods of this structure:
///
/// Ethereum signed message produced by most clients contains v where v = 27 + recovery_id(0,1,2,3),
/// but for some clients v = recovery_id(0,1,2,3).
/// Library that we use for signature verification (written for bitcoin) expects v = recovery_id
///
/// That is why:
/// 1) when we create this structure by deserialization of message produced by user
///    we subtract 27 from v in `ETHSignature` if necessary and store it in the `ETHSignature` structure this way.
/// 2) When we serialize/create this structure we add 27 to v in `ETHSignature`.
///
/// This way when we have methods that consumes &self we can be sure that ETHSignature::recover_signer works
/// And we can be sure that we are compatible with Ethereum clients.
///
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackedEthSignature(ETHSignature);

impl PackedEthSignature {
    pub fn serialize_packed(&self) -> [u8; 65] {
        // adds 27 to v
        self.0.clone().into_electrum()
    }

    fn deserialize_signature(bytes: &[u8]) -> Result<[u8; 65], DeserializeError> {
        if bytes.len() != 65 {
            return Err(DeserializeError::IncorrectSignatureLength);
        }

        let mut bytes_array = [0u8; 65];
        bytes_array.copy_from_slice(bytes);
        Ok(bytes_array)
    }

    pub fn deserialize_packed(bytes: &[u8]) -> Result<Self, DeserializeError> {
        let mut signature = Self::deserialize_signature(bytes)?;
        if signature[64] >= 27 {
            signature[64] -= 27;
        }

        Ok(PackedEthSignature(ETHSignature::from(signature)))
    }

    /// Unlike the `deserialize_packed` packed signature, this method does not make sure that the `v` value is in the range [0, 3].
    /// This one should be generally avoided and be used only in places where preservation of the original `v` is important.
    pub fn deserialize_packed_no_v_check(bytes: &[u8]) -> Result<Self, DeserializeError> {
        let signature = Self::deserialize_signature(bytes)?;
        Ok(PackedEthSignature(ETHSignature::from(signature)))
    }

    pub fn sign_raw(
        private_key: &K256PrivateKey,
        signed_bytes: &H256,
    ) -> Result<PackedEthSignature, ParityCryptoError> {
        let signature = sign(private_key, signed_bytes)?;
        Ok(PackedEthSignature(signature))
    }

    /// Signs typed struct using Ethereum private key by EIP-712 signature standard.
    /// Result of this function is the equivalent of RPC calling `eth_signTypedData`.
    pub fn sign_typed_data(
        private_key: &K256PrivateKey,
        domain: &Eip712Domain,
        typed_struct: &impl EIP712TypedStructure,
    ) -> Result<PackedEthSignature, ParityCryptoError> {
        let signed_bytes = H256::from(Self::typed_data_to_signed_bytes(domain, typed_struct).0);
        let signature = sign(private_key, &signed_bytes)?;
        Ok(PackedEthSignature(signature))
    }

    pub fn typed_data_to_signed_bytes(
        domain: &Eip712Domain,
        typed_struct: &impl EIP712TypedStructure,
    ) -> H256 {
        let mut bytes = Vec::new();
        bytes.extend_from_slice("\x19\x01".as_bytes());
        bytes.extend_from_slice(domain.hash_struct().as_bytes());
        bytes.extend_from_slice(typed_struct.hash_struct().as_bytes());
        keccak256(&bytes).into()
    }

    pub fn message_to_signed_bytes(msg: &[u8]) -> H256 {
        keccak256(msg).into()
    }

    /// Checks signature and returns Ethereum address of the signer.
    /// message should be the same message that was passed to `eth.sign`(or similar) method
    /// as argument. No hashing and prefixes required.
    pub fn signature_recover_signer(
        &self,
        signed_bytes: &H256,
    ) -> Result<Address, ParityCryptoError> {
        let signed_bytes = H256::from_slice(&signed_bytes.0);
        let public_key = recover(&self.0, &signed_bytes)?;
        let address = public_to_address(&public_key);
        Ok(Address::from(address.0))
    }

    pub fn from_rsv(r: &H256, s: &H256, v: u8) -> Self {
        let r = H256::from_slice(&r.0);
        let s = H256::from_slice(&s.0);
        PackedEthSignature(ETHSignature::from_rsv(&r, &s, v))
    }

    pub fn r(&self) -> &[u8] {
        self.0.r()
    }
    pub fn s(&self) -> &[u8] {
        self.0.s()
    }
    pub fn v(&self) -> u8 {
        self.0.v()
    }
    pub fn v_with_chain_id(&self, chain_id: u64) -> u64 {
        self.0.v() as u64 + 35 + chain_id * 2
    }
    pub fn unpack_v(v: u64) -> Result<(u8, Option<u64>), ParityCryptoError> {
        if v == 27 {
            return Ok((0, None));
        } else if v == 28 {
            return Ok((1, None));
        } else if v >= 35 {
            let chain_id = (v - 35) >> 1;
            let v = v - 35 - chain_id * 2;
            if v == 0 {
                return Ok((0, Some(chain_id)));
            } else if v == 1 {
                return Ok((1, Some(chain_id)));
            }
        }

        Err(ParityCryptoError::Custom(anyhow::format_err!("Invalid v")))
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum DeserializeError {
    #[error("Eth signature length should be 65 bytes")]
    IncorrectSignatureLength,
}

impl Serialize for PackedEthSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let packed_signature = self.serialize_packed();
        ZeroPrefixHexSerde::serialize(&packed_signature, serializer)
    }
}

impl<'de> Deserialize<'de> for PackedEthSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = ZeroPrefixHexSerde::deserialize(deserializer)?;
        Self::deserialize_packed(&bytes).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpack_v_0() {
        assert_eq!(PackedEthSignature::unpack_v(27).unwrap(), (0, None));
    }

    #[test]
    fn unpack_v_1() {
        assert_eq!(PackedEthSignature::unpack_v(28).unwrap(), (1, None));
    }

    #[test]
    fn unpack_wrong_v_10_without_chain_id() {
        assert!(PackedEthSignature::unpack_v(10).is_err());
    }

    #[test]
    fn unpack_wrong_v_30_without_chain_id() {
        assert!(PackedEthSignature::unpack_v(30).is_err());
    }

    #[test]
    fn unpack_v_0_with_chain_id_0() {
        assert_eq!(PackedEthSignature::unpack_v(35).unwrap(), (0, Some(0)));
    }

    #[test]
    fn unpack_v_1_with_chain_id_0() {
        assert_eq!(PackedEthSignature::unpack_v(36).unwrap(), (1, Some(0)));
    }

    #[test]
    fn unpack_v_1_with_chain_id_11() {
        assert_eq!(PackedEthSignature::unpack_v(58).unwrap(), (1, Some(11)));
    }

    #[test]
    fn unpack_v_1_with_chain_id_270() {
        assert_eq!(PackedEthSignature::unpack_v(576).unwrap(), (1, Some(270)));
    }
}
