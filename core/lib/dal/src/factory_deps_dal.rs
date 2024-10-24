use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use zksync_contracts::{BaseSystemContracts, SystemContractCode};
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{L2BlockNumber, ProtocolVersionId, H256, U256};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, bytes_to_chunks};

use crate::{Core, CoreDal};

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
        block_number: L2BlockNumber,
        factory_deps: &HashMap<H256, Vec<u8>>,
    ) -> DalResult<()> {
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
                UNNEST($1::bytea [], $2::bytea []) AS u (bytecode_hash, bytecode)
            ON CONFLICT (bytecode_hash) DO NOTHING
            "#,
            &bytecode_hashes as &[&[u8]],
            &bytecodes as &[&[u8]],
            i64::from(block_number.0)
        )
        .instrument("insert_factory_deps")
        .with_arg("block_number", &block_number)
        .with_arg("factory_deps.len", &factory_deps.len())
        .execute(self.storage)
        .await?;

        Ok(())
    }

    /// Returns bytecode for a factory dependency with the specified bytecode `hash`.
    /// Returns bytecodes only from sealed miniblocks.
    pub async fn get_sealed_factory_dep(&mut self, hash: H256) -> DalResult<Option<Vec<u8>>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                bytecode
            FROM
                factory_deps
            WHERE
                bytecode_hash = $1
                AND miniblock_number <= COALESCE(
                    (
                        SELECT
                            MAX(number)
                        FROM
                            miniblocks
                    ),
                    (
                        SELECT
                            miniblock_number
                        FROM
                            snapshot_recovery
                    )
                )
            "#,
            hash.as_bytes(),
        )
        .instrument("get_sealed_factory_dep")
        .with_arg("hash", &hash)
        .fetch_optional(self.storage)
        .await?
        .map(|row| row.bytecode))
    }

    async fn get_base_system_contracts_from_factory_deps(
        &mut self,
        bootloader_hash: H256,
        default_aa_hash: H256,
    ) -> anyhow::Result<Option<BaseSystemContracts>> {
        let bootloader_bytecode = self
            .get_sealed_factory_dep(bootloader_hash)
            .await
            .context("failed loading bootloader code")?;

        let default_aa_bytecode = self
            .get_sealed_factory_dep(default_aa_hash)
            .await
            .context("failed loading default account code")?;

        let (Some(bootloader_bytecode), Some(default_aa_bytecode)) =
            (bootloader_bytecode, default_aa_bytecode)
        else {
            return Ok(None);
        };

        let bootloader_code = SystemContractCode {
            code: bytes_to_be_words(bootloader_bytecode),
            hash: bootloader_hash,
        };

        let default_aa_code = SystemContractCode {
            code: bytes_to_be_words(default_aa_bytecode),
            hash: default_aa_hash,
        };
        Ok(Some(BaseSystemContracts {
            bootloader: bootloader_code,
            default_aa: default_aa_code,
        }))
    }

    pub async fn get_base_system_contracts(
        &mut self,
        protocol_version: Option<ProtocolVersionId>,
        bootloader_hash: H256,
        default_aa_hash: H256,
    ) -> anyhow::Result<BaseSystemContracts> {
        // There are two potential sources of base contracts bytecode:
        // - Factory deps of the upgrade transaction
        // - Database

        // Firstly trying from factory deps
        if let Some(deps) = self
            .get_base_system_contracts_from_factory_deps(bootloader_hash, default_aa_hash)
            .await?
        {
            return Ok(deps);
        }

        println!("Hi!!!2");

        let error_msg = format!("Could not find either of the following factoey deps: bootloader {bootloader_hash:?} or {default_aa_hash:?}");

        let Some(protocol_version) = protocol_version else {
            // TODO: refactor
            return None.context(error_msg);
        };

        println!("Hi!!!3");

        let upgrade_tx = self
            .storage
            .protocol_versions_dal()
            .get_protocol_upgrade_tx(protocol_version)
            .await?
            .context(error_msg.clone())?;

        println!("Hi!!!10");
        if upgrade_tx.execute.factory_deps.len() < 2 {
            // TODO: refactor
            return None.context(error_msg);
        }

        println!("Hi!!!");

        let bootloader_preimage = upgrade_tx.execute.factory_deps[0].clone();
        let default_aa_preimage = upgrade_tx.execute.factory_deps[1].clone();

        if hash_bytecode(&bootloader_preimage) != bootloader_hash
            || hash_bytecode(&default_aa_preimage) != default_aa_hash
        {
            // TODO: refactor
            return None.context(error_msg);
        }

        println!("Hi1212121!!!");

        Ok(BaseSystemContracts {
            bootloader: SystemContractCode {
                code: bytes_to_be_words(bootloader_preimage),
                hash: bootloader_hash,
            },
            default_aa: SystemContractCode {
                code: bytes_to_be_words(default_aa_preimage),
                hash: default_aa_hash,
            },
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
                bytecode_hash = ANY($1)
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
        block_number: L2BlockNumber,
    ) -> DalResult<Vec<H256>> {
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
        .instrument("get_factory_deps_for_revert")
        .with_arg("block_number", &block_number)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|row| H256::from_slice(&row.bytecode_hash))
        .collect())
    }

    /// Removes all factory deps with a miniblock number strictly greater than the specified `block_number`.
    pub async fn roll_back_factory_deps(&mut self, block_number: L2BlockNumber) -> DalResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM factory_deps
            WHERE
                miniblock_number > $1
            "#,
            i64::from(block_number.0)
        )
        .instrument("roll_back_factory_deps")
        .with_arg("block_number", &block_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// Retrieves all factory deps entries for testing purposes.
    pub async fn dump_all_factory_deps_for_tests(&mut self) -> HashMap<H256, Vec<u8>> {
        sqlx::query!(
            r#"
            SELECT
                bytecode,
                bytecode_hash
            FROM
                factory_deps
            "#
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (H256::from_slice(&row.bytecode_hash), row.bytecode))
        .collect()
    }
}
