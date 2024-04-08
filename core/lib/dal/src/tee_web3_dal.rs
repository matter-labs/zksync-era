use secp256k1::PublicKey;
use zksync_types::api::Attestation;

use crate::{SqlxError, StorageProcessor};

#[derive(Debug)]
pub struct TeeWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl TeeWeb3Dal<'_, '_> {
    pub async fn get_attestation(&mut self, pk: PublicKey) -> Result<Attestation, SqlxError> {
        let attestation = sqlx::query_as!(
            Attestation,
            r#"
            SELECT
                attestation
            FROM
                sgx_attestation
            WHERE
                public_key = $1
            "#,
            &pk.serialize()
        )
        .fetch_one(self.storage.conn())
        .await?;
        Ok(attestation)
    }

    pub async fn set_attestation(
        &mut self,
        key: &secp256k1::PublicKey,
        quote: &Vec<u8>,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                sgx_attestation (public_key, attestation)
            VALUES
                ($1, $2)
            "#,
            &key.serialize(),
            quote
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }
}
