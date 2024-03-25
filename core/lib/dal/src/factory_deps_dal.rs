use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use zksync_contracts::{BaseSystemContracts, SystemContractCode};
use zksync_db_connection::connection::Connection;
use zksync_types::{MiniblockNumber, H256, U256};
use zksync_utils::{bytes_to_be_words, bytes_to_chunks};

use crate::Core;

/// DAL methods related to factory dependencies.
#[derive(Debug)]
pub struct FactoryDepsDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl FactoryDepsDal<'_, '_> {
    /// Inserts factory dependencies for a miniblock. Factory deps are specified as a map of
    /// `(bytecode_hash, bytecode)` entries.
    pub async fn insert_factory_deps(
        &mut self,
        block_number: MiniblockNumber,
        factory_deps: &HashMap<H256, Vec<u8>>,
    ) -> sqlx::Result<()> {
        let (bytecode_hashes, bytecodes): (Vec<_>, Vec<_>) = factory_deps
            .iter()
            .map(|(hash, bytecode)| (hash.as_bytes(), bytecode.as_slice()))
            .unzip();

        // Copy from stdin can't be used here because of `ON CONFLICT`.
        sqlx::query!(
            r#"
            INSERT INTO
                factory_deps (bytecode_hash, bytecode, miniblock_number, created_at, updated_at)
            SELECT
                u.bytecode_hash,
                u.bytecode,
                $3,
                NOW(),
                NOW()
            FROM
                UNNEST($1::bytea[], $2::bytea[]) AS u (bytecode_hash, bytecode)
            ON CONFLICT (bytecode_hash) DO NOTHING
            "#,
            &bytecode_hashes as &[&[u8]],
            &bytecodes as &[&[u8]],
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await?;

        Ok(())
    }

    /// Returns bytecode for a factory dependency with the specified bytecode `hash`.
    pub async fn get_factory_dep(&mut self, hash: H256) -> sqlx::Result<Option<Vec<u8>>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                bytecode
            FROM
                factory_deps
            WHERE
                bytecode_hash = $1
            "#,
            hash.as_bytes(),
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.bytecode))
    }

    pub async fn get_base_system_contracts(
        &mut self,
        bootloader_hash: H256,
        default_aa_hash: H256,
    ) -> anyhow::Result<BaseSystemContracts> {
        let bootloader_bytecode = self
            .get_factory_dep(bootloader_hash)
            .await
            .context("failed loading bootloader code")?
            .with_context(|| format!("bootloader code with hash {bootloader_hash:?} should be present in the database"))?;
        let bootloader_code = SystemContractCode {
            code: bytes_to_be_words(bootloader_bytecode),
            hash: bootloader_hash,
        };

        let default_aa_bytecode = self
            .get_factory_dep(default_aa_hash)
            .await
            .context("failed loading default account code")?
            .with_context(|| format!("default account code with hash {default_aa_hash:?} should be present in the database"))?;

        let default_aa_code = SystemContractCode {
            code: bytes_to_be_words(default_aa_bytecode),
            hash: default_aa_hash,
        };
        Ok(BaseSystemContracts {
            bootloader: bootloader_code,
            default_aa: default_aa_code,
        })
    }

    /// Returns bytecodes for factory deps with the specified `hashes`.
    pub async fn get_factory_deps(
        &mut self,
        hashes: &HashSet<H256>,
    ) -> HashMap<U256, Vec<[u8; 32]>> {
        let hashes_as_bytes: Vec<_> = hashes.iter().map(H256::as_bytes).collect();

        sqlx::query!(
            r#"
            SELECT
                bytecode,
                bytecode_hash
            FROM
                factory_deps
            WHERE
                bytecode_hash = ANY ($1)
            "#,
            &hashes_as_bytes as &[&[u8]],
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            (
                U256::from_big_endian(&row.bytecode_hash),
                bytes_to_chunks(&row.bytecode),
            )
        })
        .collect()
    }

    /// Returns bytecode hashes for factory deps from miniblocks with number strictly greater
    /// than `block_number`.
    pub async fn get_factory_deps_for_revert(
        &mut self,
        block_number: MiniblockNumber,
    ) -> sqlx::Result<Vec<H256>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                bytecode_hash
            FROM
                factory_deps
            WHERE
                miniblock_number > $1
            "#,
            i64::from(block_number.0)
        )
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(|row| H256::from_slice(&row.bytecode_hash))
        .collect())
    }

    /// Removes all factory deps with a miniblock number strictly greater than the specified `block_number`.
    pub async fn rollback_factory_deps(
        &mut self,
        block_number: MiniblockNumber,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM factory_deps
            WHERE
                miniblock_number > $1
            "#,
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }
}
