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

// Copied from parity-crypto 0.9.0 src/publickey/keypair.rs
// Key pair (public + secret) description.

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

use crate::web3::{
    signing::keccak256,
    types::{Address, H256, H512, H520},
};

type Message = H256;
type Public = H512;
type Secret = H256;

/// Convert public key into the address
pub(super) fn public_to_address(public: &Public) -> Address {
    let hash = keccak256(public.as_bytes());
    let mut result = Address::zero();
    result.as_bytes_mut().copy_from_slice(&hash[12..]);
    result
}

#[derive(Debug, Clone, PartialEq)]
/// secp256k1 key pair
pub(super) struct KeyPair {
    secret: Secret,
    public: Public,
}

impl fmt::Display for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        writeln!(f, "secret:  {:x}", self.secret)?;
        writeln!(f, "public:  {:x}", self.public)?;
        write!(f, "address: {:x}", self.address())
    }
}

impl KeyPair {
    /// Create a pair from secret key
    pub(super) fn from_secret(secret: Secret) -> Result<KeyPair, Error> {
        let context = &SECP256K1;
        let s: SecretKey = SecretKey::from_slice(secret.as_bytes())?;
        let pub_key = PublicKey::from_secret_key(context, &s);
        let serialized = pub_key.serialize_uncompressed();

        let mut public = Public::default();
        public.as_bytes_mut().copy_from_slice(&serialized[1..65]);

        let keypair = KeyPair { secret, public };

        Ok(keypair)
    }

    /// Returns public part of the keypair converted into Address
    pub(super) fn address(&self) -> Address {
        public_to_address(&self.public)
    }
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
pub(super) struct Signature([u8; 65]);

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
    pub(super) fn into_electrum(mut self) -> [u8; 65] {
        self.0[64] += 27;
        self.0
    }

    /// Parse bytes as a signature encoded as RSV (V in "Electrum" notation).
    /// May return empty (invalid) signature if given data has invalid length.
    #[cfg(test)]
    pub(super) fn from_electrum(data: &[u8]) -> Self {
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
pub(super) fn sign(secret: &Secret, message: &Message) -> Result<Signature, Error> {
    let context = &SECP256K1;
    let sec = SecretKey::from_slice(secret.as_ref())?;
    let s = context.sign_ecdsa_recoverable(&SecpMessage::from_slice(&message[..])?, &sec);
    let (rec_id, data) = s.serialize_compact();
    let mut data_arr = [0; 65];

    // no need to check if s is low, it always is
    data_arr[0..64].copy_from_slice(&data[0..64]);
    data_arr[64] = rec_id.to_i32() as u8;
    Ok(Signature(data_arr))
}

/// Recovers the public key from the signature for the message
pub(super) fn recover(signature: &Signature, message: &Message) -> Result<Public, Error> {
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
    use std::str::FromStr;

    use super::{recover, sign, KeyPair, Message, Secret, Signature};

    #[test]
    fn from_secret() {
        let secret = Secret::from_slice(
            &hex::decode("a100df7a048e50ed308ea696dc600215098141cb391e9527329df289f9383f65")
                .unwrap(),
        );
        let _ = KeyPair::from_secret(secret).unwrap();
    }

    #[test]
    fn keypair_display() {
        let expected =
"secret:  a100df7a048e50ed308ea696dc600215098141cb391e9527329df289f9383f65
public:  8ce0db0b0359ffc5866ba61903cc2518c3675ef2cf380a7e54bde7ea20e6fa1ab45b7617346cd11b7610001ee6ae5b0155c41cad9527cbcdff44ec67848943a4
address: 5b073e9233944b5e729e46d618f0d8edf3d9c34a".to_owned();
        let secret = Secret::from_slice(
            &hex::decode("a100df7a048e50ed308ea696dc600215098141cb391e9527329df289f9383f65")
                .unwrap(),
        );
        let kp = KeyPair::from_secret(secret).unwrap();
        assert_eq!(format!("{}", kp), expected);
    }

    #[test]
    fn vrs_conversion() {
        // given
        let secret = Secret::from_slice(&[1u8; 32]);
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
        let secret = Secret::from_slice(&[1u8; 32]);
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
        let secret = Secret::from_slice(&[1u8; 32]);
        let keypair = KeyPair::from_secret(secret).unwrap();
        let message =
            Message::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let signature = sign(&keypair.secret, &message).unwrap();
        assert_eq!(&keypair.public, &recover(&signature, &message).unwrap());
    }

    #[test]
    fn sign_and_recover_public_works_with_zeroed_messages() {
        let secret = Secret::from_slice(&[1u8; 32]);
        let keypair = KeyPair::from_secret(secret).unwrap();
        let signature = sign(&keypair.secret, &Message::zero()).unwrap();
        let zero_message = Message::zero();
        assert_eq!(
            &keypair.public,
            &recover(&signature, &zero_message).unwrap()
        );
    }

    #[test]
    fn recover_allowing_all_zero_message_can_recover_from_all_zero_messages() {
        let secret = Secret::from_slice(&[1u8; 32]);
        let keypair = KeyPair::from_secret(secret).unwrap();
        let signature = sign(&keypair.secret, &Message::zero()).unwrap();
        let zero_message = Message::zero();
        assert_eq!(
            &keypair.public,
            &recover(&signature, &zero_message).unwrap()
        )
    }
}
