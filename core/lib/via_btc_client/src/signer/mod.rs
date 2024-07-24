use async_trait::async_trait;
use bitcoin::consensus::deserialize;
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::{All, Message, Secp256k1};
use bitcoin::secp256k1;
use bitcoin::sighash::{EcdsaSighashType, Prevouts, SighashCache, TapSighashType};
use bitcoin::taproot::{ControlBlock, LeafVersion};
use bitcoin::{Amount, PrivateKey, Script, ScriptBuf, TapLeafHash, Transaction, Witness};
use crate::traits::BitcoinSigner;
use crate::types::{BitcoinSignerResult, BitcoinError};

struct BitcoinSignerImpl {
    private_key: PrivateKey,
    secp: Secp256k1<All>,
}

#[async_trait]
impl BitcoinSigner for BitcoinSignerImpl {
    async fn new(private_key: &str) -> BitcoinSignerResult<Self> {
        let private_key = PrivateKey::from_wif(private_key)
            .map_err(|e| BitcoinError::InvalidPrivateKey(e.to_string()))?;
        let secp = Secp256k1::new();
        Ok(Self { private_key, secp })
    }

    async fn sign_transaction(
        &self,
        unsigned_transaction: &str,
    ) -> BitcoinSignerResult<String> {
        let mut unsigned_tx: Transaction = deserialize(&hex::decode(unsigned_transaction)
            .map_err(|e| BitcoinError::InvalidTransaction(e.to_string()))?)
            .map_err(|e| BitcoinError::InvalidTransaction(e.to_string()))?;

        let keypair = self.private_key.inner.keypair(&self.secp);
        let (internal_key, _parity) = keypair.x_only_public_key();

        let mut sighasher = SighashCache::new(&mut unsigned_tx);

        // fee

        // TODO: it seems we need to get ScriptPubKey and Value from rpc?
        let fee_input_index = 0;
        let sighash_type = EcdsaSighashType::All;
        let fee_input_sighash = sighasher
            .p2wpkh_signature_hash(
                fee_input_index,
                Script::new(), // TODO
                Amount::default(),
                sighash_type,
            )
            .map_err(|e| BitcoinError::SigningError(e.to_string()))?;

        let msg = Message::from(fee_input_sighash);
        let fee_input_signature = self.secp.sign_ecdsa(&msg, &self.private_key.inner);

        let fee_input_signature = bitcoin::ecdsa::Signature {
            signature: fee_input_signature,
            sighash_type,
        };
        let pk: secp256k1::PublicKey = self.private_key.public_key(&self.secp);


        *sighasher.witness_mut(fee_input_index).unwrap() = Witness::p2wpkh(&fee_input_signature, &pk);

        // reveal

        let reveal_input_index = 1;
        let sighash_type = TapSighashType::All;

        // TODO: correct prevouts
        let prevouts = Prevouts::All(&[]);
        // TODO: taproot
        let taproot_script = ScriptBuf::new();

        let reveal_input_sighash = sighasher
            .taproot_script_spend_signature_hash(
                reveal_input_index,
                &prevouts,
                TapLeafHash::from_script(&taproot_script, LeafVersion::TapScript),
                sighash_type,
            )
            .map_err(|e| BitcoinError::SigningError(e.to_string()))?;

        let msg = Message::from_digest(reveal_input_sighash.to_byte_array());
        let reveal_input_signature = self.secp.sign_schnorr_no_aux_rand(&msg, &keypair);

        let reveal_input_signature = bitcoin::taproot::Signature {
            signature: reveal_input_signature,
            sighash_type,
        };

        let mut witness_data = Witness::new();
        witness_data.push(&reveal_input_signature.to_vec());
        witness_data.push(&taproot_script.to_bytes());

        // TODO: change control block
        let control_block = ControlBlock::decode(&[0u8]).map_err(|e| BitcoinError::SigningError(e.to_string()))?;
        witness_data.push(&control_block.serialize());

        *sighasher.witness_mut(reveal_input_index).unwrap() = witness_data;

        let signed_tx = sighasher.into_transaction();

        Ok(hex::encode(bitcoin::consensus::serialize(&signed_tx)))
    }
}