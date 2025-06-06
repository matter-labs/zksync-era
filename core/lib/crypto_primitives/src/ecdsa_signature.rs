// Mostly copy-pasted from parity-crypto 0.9.0:
// https://github.com/paritytech/parity-common/blob/parity-crypto-v0.9.0/parity-crypto/src/publickey/keypair.rs
// https://github.com/paritytech/parity-common/blob/parity-crypto-v0.9.0/parity-crypto/src/publickey/ecdsa_signature.rs
//
// Reason: parity-crypto crate is not maintained, and it provides convenience wrappers over secp256k1 we rely on.
// For the time being, vendoring these files is more convenient than rewriting the rest of the codebase to use
// secp256k1 directly.
//
// Changes made: adapting the code for the newer version of secp256k1, stripping down some code we don't need,
// type replacements for the ease of use.

// Copyright 2020 Parity Technologies
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Partially copied from parity-crypto 0.9.0 src/publickey/keypair.rs

use std::{
    cmp::PartialEq,
    fmt,
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
    str::FromStr,
};

use secp256k1::{
    ecdsa::{RecoverableSignature, RecoveryId},
    Message as SecpMessage, PublicKey, SecretKey, SECP256K1,
};
use zksync_basic_types::{
    web3::{self, keccak256},
    Address, H256, H512, H520,
};

type Message = H256;
type Public = H512;

/// secp256k1 private key wrapper.
///
/// Provides a safe to use `Debug` implementation (outputting the address corresponding to the key).
/// The key is zeroized on drop.
#[derive(Clone, PartialEq)]
pub struct K256PrivateKey(SecretKey);

impl fmt::Debug for K256PrivateKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Secret")
            .field("address", &self.address())
            .finish()
    }
}

impl From<SecretKey> for K256PrivateKey {
    fn from(secret_key: SecretKey) -> Self {
        K256PrivateKey(secret_key)
    }
}

impl K256PrivateKey {
    /// Converts a 32-byte array into a key.
    ///
    /// # Errors
    ///
    /// Returns an error if the deserialized scalar (as a big-endian number) is zero or is greater or equal
    /// than the secp256k1 group order. The probability of this is negligible if the bytes are random.
    pub fn from_bytes(bytes: H256) -> Result<Self, Error> {
        Ok(Self(SecretKey::from_slice(bytes.as_bytes())?))
    }

    /// Generates a random private key using the OS RNG.
    pub fn random() -> Self {
        Self::random_using(&mut rand::rngs::OsRng)
    }

    /// Generates a random private key using the provided RNG.
    pub fn random_using(rng: &mut impl rand::Rng) -> Self {
        loop {
            if let Ok(this) = Self::from_bytes(H256::random_using(rng)) {
                return this;
            }
        }
    }

    /// Exposes the underlying secret key. This is the only way to get it.
    pub fn expose_secret(&self) -> &SecretKey {
        &self.0
    }

    /// Returns uncompressed bytes for the secp256k1 public key corresponding to this key, without
    /// the first key type byte.
    pub fn public(&self) -> Public {
        let pub_key = PublicKey::from_secret_key(SECP256K1, &self.0);
        let mut public = Public::zero();
        public
            .as_bytes_mut()
            .copy_from_slice(&pub_key.serialize_uncompressed()[1..]);
        public
    }

    /// Returns the Ethereum-like address corresponding to this key.
    pub fn address(&self) -> Address {
        let pub_key = PublicKey::from_secret_key(SECP256K1, &self.0);
        let hash = keccak256(&pub_key.serialize_uncompressed()[1..]);
        let mut result = Address::zero();
        result.as_bytes_mut().copy_from_slice(&hash[12..]);
        result
    }

    pub fn sign_web3(&self, message: &H256, chain_id: Option<u64>) -> web3::Signature {
        let message = secp256k1::Message::from_slice(message.as_bytes()).unwrap();
        let (recovery_id, signature) = SECP256K1
            .sign_ecdsa_recoverable(&message, &self.0)
            .serialize_compact();

        let standard_v = recovery_id.to_i32() as u64;
        let v = if let Some(chain_id) = chain_id {
            // When signing with a chain ID, add chain replay protection.
            standard_v + 35 + chain_id * 2
        } else {
            // Otherwise, convert to 'Electrum' notation.
            standard_v + 27
        };
        let r = H256::from_slice(&signature[..32]);
        let s = H256::from_slice(&signature[32..]);

        web3::Signature { r, s, v }
    }

    pub fn sign_web3_message(&self, message: &H256) -> web3::Signature {
        let message = secp256k1::Message::from_slice(message.as_bytes()).unwrap();
        let (recovery_id, signature) = SECP256K1
            .sign_ecdsa_recoverable(&message, &self.0)
            .serialize_compact();

        let v = recovery_id.to_i32() as u64;
        let r = H256::from_slice(&signature[..32]);
        let s = H256::from_slice(&signature[32..]);

        web3::Signature { v, r, s }
    }
}

/// Convert public key into the address
pub fn public_to_address(public: &Public) -> Address {
    let hash = keccak256(public.as_bytes());
    let mut result = Address::zero();
    result.as_bytes_mut().copy_from_slice(&hash[12..]);
    result
}

// Copied from parity-crypto 0.9.0 `src/publickey/ecdsa_signature.rs`

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("secp256k1 error: {0}")]
    Secp256k1(#[from] secp256k1::Error),
    #[error("invalid signature")]
    InvalidSignature,
    #[error(transparent)]
    Custom(anyhow::Error),
}

/// Signature encoded as RSV components
#[repr(C)]
pub struct Signature([u8; 65]);

impl Signature {
    /// Get a slice into the 'r' portion of the data.
    pub(super) fn r(&self) -> &[u8] {
        &self.0[0..32]
    }

    /// Get a slice into the 's' portion of the data.
    pub(super) fn s(&self) -> &[u8] {
        &self.0[32..64]
    }

    /// Get the recovery byte.
    pub(super) fn v(&self) -> u8 {
        self.0[64]
    }

    /// Encode the signature into RSV array (V altered to be in "Electrum" notation).
    pub fn into_electrum(mut self) -> [u8; 65] {
        self.0[64] += 27;
        self.0
    }

    /// Parse bytes as a signature encoded as RSV (V in "Electrum" notation).
    /// May return empty (invalid) signature if given data has invalid length.
    pub fn from_electrum(data: &[u8]) -> Self {
        if data.len() != 65 || data[64] < 27 {
            // fallback to empty (invalid) signature
            return Signature::default();
        }

        let mut sig = [0u8; 65];
        sig.copy_from_slice(data);
        sig[64] -= 27;
        Signature(sig)
    }

    /// Create a signature object from the RSV triple.
    pub(super) fn from_rsv(r: &H256, s: &H256, v: u8) -> Self {
        let mut sig = [0u8; 65];
        sig[0..32].copy_from_slice(r.as_ref());
        sig[32..64].copy_from_slice(s.as_ref());
        sig[64] = v;
        Signature(sig)
    }
}

// manual implementation large arrays don't have trait impls by default.
impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

// manual implementation required in Rust 1.13+, see `std::cmp::AssertParamIsEq`.
impl Eq for Signature {}

// also manual for the same reason, but the pretty printing might be useful.
impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        f.debug_struct("Signature")
            .field("r", &hex::encode(&self.0[0..32]))
            .field("s", &hex::encode(&self.0[32..64]))
            .field("v", &hex::encode(&self.0[64..65]))
            .finish()
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl FromStr for Signature {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match hex::decode(s) {
            Ok(ref hex) if hex.len() == 65 => {
                let mut data = [0; 65];
                data.copy_from_slice(&hex[0..65]);
                Ok(Signature(data))
            }
            _ => Err(Error::InvalidSignature),
        }
    }
}

impl Default for Signature {
    fn default() -> Self {
        Signature([0; 65])
    }
}

impl Hash for Signature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        H520::from(self.0).hash(state);
    }
}

impl Clone for Signature {
    fn clone(&self) -> Self {
        Signature(self.0)
    }
}

impl From<[u8; 65]> for Signature {
    fn from(s: [u8; 65]) -> Self {
        Signature(s)
    }
}

impl From<Signature> for H520 {
    fn from(s: Signature) -> Self {
        H520::from(s.0)
    }
}

impl From<H520> for Signature {
    fn from(bytes: H520) -> Self {
        Signature(bytes.into())
    }
}

impl Deref for Signature {
    type Target = [u8; 65];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Signature {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Signs message with the given secret key.
/// Returns the corresponding signature.
pub fn sign(secret: &K256PrivateKey, message: &Message) -> Result<Signature, Error> {
    let context = &SECP256K1;
    let s = context.sign_ecdsa_recoverable(&SecpMessage::from_slice(&message[..])?, &secret.0);
    let (rec_id, data) = s.serialize_compact();
    let mut data_arr = [0; 65];

    // no need to check if s is low, it always is
    data_arr[0..64].copy_from_slice(&data[0..64]);
    data_arr[64] = rec_id.to_i32() as u8;
    Ok(Signature(data_arr))
}

/// Recovers the public key from the signature for the message
pub fn recover(signature: &Signature, message: &Message) -> Result<Public, Error> {
    let rsig = RecoverableSignature::from_compact(
        &signature[0..64],
        RecoveryId::from_i32(signature[64] as i32)?,
    )?;
    let pubkey = &SECP256K1.recover_ecdsa(&SecpMessage::from_slice(&message[..])?, &rsig)?;
    let serialized = pubkey.serialize_uncompressed();
    let mut public = Public::default();
    public.as_bytes_mut().copy_from_slice(&serialized[1..65]);
    Ok(public)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_conversions() {
        let private_key_bytes: H256 =
            "a100df7a048e50ed308ea696dc600215098141cb391e9527329df289f9383f65"
                .parse()
                .unwrap();
        let private_key = K256PrivateKey::from_bytes(private_key_bytes).unwrap();
        assert_eq!(
            private_key.public(),
            "8ce0db0b0359ffc5866ba61903cc2518c3675ef2cf380a7e54bde7ea20e6fa1a\
             b45b7617346cd11b7610001ee6ae5b0155c41cad9527cbcdff44ec67848943a4"
                .parse()
                .unwrap()
        );
        assert_eq!(
            private_key.address(),
            "5b073e9233944b5e729e46d618f0d8edf3d9c34a".parse().unwrap()
        );
    }

    #[test]
    fn key_is_not_exposed_in_debug_impl() {
        let private_key_str = "a100df7a048e50ed308ea696dc600215098141cb391e9527329df289f9383f65";
        let private_key_bytes: H256 = private_key_str.parse().unwrap();
        let private_key = K256PrivateKey::from_bytes(private_key_bytes).unwrap();
        let private_key_debug = format!("{private_key:?}");
        assert!(
            !private_key_debug.contains(private_key_str),
            "{private_key_debug}"
        );
        assert!(
            private_key_debug.contains("5b073e9233944b5e729e46d618f0d8edf3d9c34a"),
            "{private_key_debug}"
        );
    }

    #[test]
    fn vrs_conversion() {
        // given
        let secret = K256PrivateKey::from_bytes(H256::repeat_byte(1)).unwrap();
        let message =
            Message::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let signature = sign(&secret, &message).expect("can sign a non-zero message");

        // when
        let vrs = signature.clone().into_electrum();
        let from_vrs = Signature::from_electrum(&vrs);

        // then
        assert_eq!(signature, from_vrs);
    }

    #[test]
    fn signature_to_and_from_str() {
        let secret = K256PrivateKey::from_bytes(H256::repeat_byte(1)).unwrap();
        let message =
            Message::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let signature = sign(&secret, &message).expect("can sign a non-zero message");
        let string = format!("{}", signature);
        let deserialized = Signature::from_str(&string).unwrap();
        assert_eq!(signature, deserialized);
    }

    #[test]
    fn sign_and_recover_public() {
        let secret = K256PrivateKey::from_bytes(H256::repeat_byte(1)).unwrap();
        let message =
            Message::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let signature = sign(&secret, &message).unwrap();
        assert_eq!(secret.public(), recover(&signature, &message).unwrap());
    }

    #[test]
    fn sign_and_recover_public_works_with_zeroed_messages() {
        let secret = K256PrivateKey::from_bytes(H256::repeat_byte(1)).unwrap();
        let signature = sign(&secret, &Message::zero()).unwrap();
        let zero_message = Message::zero();
        assert_eq!(secret.public(), recover(&signature, &zero_message).unwrap());
    }

    #[test]
    fn recover_allowing_all_zero_message_can_recover_from_all_zero_messages() {
        let secret = K256PrivateKey::from_bytes(H256::repeat_byte(1)).unwrap();
        let signature = sign(&secret, &Message::zero()).unwrap();
        let zero_message = Message::zero();
        assert_eq!(secret.public(), recover(&signature, &zero_message).unwrap())
    }
}
