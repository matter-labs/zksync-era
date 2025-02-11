use std::convert::TryInto;

use anyhow::Context as _;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
};
use zksync_types::{
    protocol_upgrade::{ProtocolUpgradeTx, ProtocolVersion},
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion, VersionPatch},
    ProtocolVersionId, H256,
};

use crate::{
    models::{
        parse_protocol_version,
        storage_protocol_version::{protocol_version_from_storage, StorageProtocolVersion},
    },
    Core, CoreDal,
};

#[derive(Debug)]
pub struct ProtocolVersionsDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl ProtocolVersionsDal<'_, '_> {
    pub async fn save_protocol_version(
        &mut self,
        version: ProtocolSemanticVersion,
        timestamp: u64,
        l1_verifier_config: L1VerifierConfig,
        base_system_contracts_hashes: BaseSystemContractsHashes,
        tx_hash: Option<H256>,
    ) -> DalResult<()> {
        let mut db_transaction = self.storage.start_transaction().await?;

        sqlx::query!(
            r#"
            INSERT INTO
            protocol_versions (
                id,
                timestamp,
                bootloader_code_hash,
                default_account_code_hash,
                evm_emulator_code_hash,
                upgrade_tx_hash,
                created_at
            )
            VALUES
            ($1, $2, $3, $4, $5, $6, NOW())
            ON CONFLICT DO NOTHING
            "#,
            version.minor as i32,
            timestamp as i64,
            base_system_contracts_hashes.bootloader.as_bytes(),
            base_system_contracts_hashes.default_aa.as_bytes(),
            base_system_contracts_hashes
                .evm_emulator
                .as_ref()
                .map(H256::as_bytes),
            tx_hash.as_ref().map(H256::as_bytes),
        )
        .instrument("save_protocol_version#minor")
        .with_arg("minor", &version.minor)
        .with_arg(
            "base_system_contracts_hashes",
            &base_system_contracts_hashes,
        )
        .with_arg("tx_hash", &tx_hash)
        .execute(&mut db_transaction)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO
            protocol_patches (
                minor,
                patch,
                snark_wrapper_vk_hash,
                fflonk_snark_wrapper_vk_hash,
                created_at
            )
            VALUES
            ($1, $2, $3, $4, NOW())
            ON CONFLICT DO NOTHING
            "#,
            version.minor as i32,
            version.patch.0 as i32,
            l1_verifier_config.snark_wrapper_vk_hash.as_bytes(),
            l1_verifier_config
                .fflonk_snark_wrapper_vk_hash
                .as_ref()
                .map(|x| x.as_bytes()),
        )
        .instrument("save_protocol_version#patch")
        .with_arg("version", &version)
        .execute(&mut db_transaction)
        .await?;

        db_transaction.commit().await?;

        Ok(())
    }

    pub async fn save_protocol_version_with_tx(
        &mut self,
        version: &ProtocolVersion,
    ) -> DalResult<()> {
        let tx_hash = version.tx.as_ref().map(|tx| tx.common_data.hash());
        let mut db_transaction = self.storage.start_transaction().await?;
        if let Some(tx) = &version.tx {
            db_transaction
                .transactions_dal()
                .insert_system_transaction(tx)
                .await?;
        }

        db_transaction
            .protocol_versions_dal()
            .save_protocol_version(
                version.version,
                version.timestamp,
                version.l1_verifier_config,
                version.base_system_contracts_hashes,
                tx_hash,
            )
            .await?;
        db_transaction.commit().await
    }

    async fn save_genesis_upgrade_tx_hash(
        &mut self,
        id: ProtocolVersionId,
        tx_hash: Option<H256>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE protocol_versions
            SET
                upgrade_tx_hash = $1
            WHERE
                id = $2
            "#,
            tx_hash.as_ref().map(H256::as_bytes),
            id as i32,
        )
        .instrument("save_genesis_upgrade_tx_hash")
        .with_arg("id", &id)
        .with_arg("tx_hash", &tx_hash)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// Attaches a transaction used to set ChainId to the genesis protocol version.
    /// Also inserts that transaction into the database.
    pub async fn save_genesis_upgrade_with_tx(
        &mut self,
        id: ProtocolVersionId,
        tx: &ProtocolUpgradeTx,
    ) -> DalResult<()> {
        let tx_hash = Some(tx.common_data.hash());
        let mut db_transaction = self.storage.start_transaction().await?;
        db_transaction
            .transactions_dal()
            .insert_system_transaction(tx)
            .await?;
        db_transaction
            .protocol_versions_dal()
            .save_genesis_upgrade_tx_hash(id, tx_hash)
            .await?;
        db_transaction.commit().await
    }

    pub async fn protocol_version_id_by_timestamp(
        &mut self,
        current_timestamp: u64,
    ) -> sqlx::Result<ProtocolVersionId> {
        let row = sqlx::query!(
            r#"
            SELECT
                id
            FROM
                protocol_versions
            WHERE
                timestamp <= $1
            ORDER BY
                id DESC
            LIMIT
                1
            "#,
            current_timestamp as i64
        )
        .fetch_one(self.storage.conn())
        .await?;

        ProtocolVersionId::try_from(row.id as u16).map_err(|err| sqlx::Error::Decode(err.into()))
    }

    /// Returns base system contracts' hashes.
    pub async fn get_base_system_contract_hashes_by_version_id(
        &mut self,
        version_id: ProtocolVersionId,
    ) -> anyhow::Result<Option<BaseSystemContractsHashes>> {
        let row = sqlx::query!(
            r#"
            SELECT
                bootloader_code_hash,
                default_account_code_hash,
                evm_emulator_code_hash
            FROM
                protocol_versions
            WHERE
                id = $1
            "#,
            i32::from(version_id as u16)
        )
        .instrument("get_base_system_contract_hashes_by_version_id")
        .with_arg("version_id", &(version_id as u16))
        .fetch_optional(self.storage)
        .await
        .context("cannot fetch system contract hashes")?;

        Ok(if let Some(row) = row {
            Some(BaseSystemContractsHashes {
                bootloader: H256::from_slice(&row.bootloader_code_hash),
                default_aa: H256::from_slice(&row.default_account_code_hash),
                evm_emulator: row.evm_emulator_code_hash.as_deref().map(H256::from_slice),
            })
        } else {
            None
        })
    }

    pub async fn get_protocol_version_with_latest_patch(
        &mut self,
        version_id: ProtocolVersionId,
    ) -> DalResult<Option<ProtocolVersion>> {
        let maybe_row = sqlx::query_as!(
            StorageProtocolVersion,
            r#"
            SELECT
                protocol_versions.id AS "minor!",
                protocol_versions.timestamp,
                protocol_versions.bootloader_code_hash,
                protocol_versions.default_account_code_hash,
                protocol_versions.evm_emulator_code_hash,
                protocol_patches.patch,
                protocol_patches.snark_wrapper_vk_hash,
                protocol_patches.fflonk_snark_wrapper_vk_hash
            FROM
                protocol_versions
            JOIN protocol_patches ON protocol_patches.minor = protocol_versions.id
            WHERE
                id = $1
            ORDER BY
                protocol_patches.patch DESC
            LIMIT
                1
            "#,
            version_id as i32
        )
        .instrument("get_protocol_version_with_latest_patch")
        .with_arg("version_id", &version_id)
        .fetch_optional(self.storage)
        .await?;

        let Some(row) = maybe_row else {
            return Ok(None);
        };
        let tx = self.get_protocol_upgrade_tx(version_id).await?;

        Ok(Some(protocol_version_from_storage(row, tx)))
    }

    pub async fn l1_verifier_config_for_version(
        &mut self,
        version: ProtocolSemanticVersion,
    ) -> Option<L1VerifierConfig> {
        let row = sqlx::query!(
            r#"
            SELECT
                snark_wrapper_vk_hash,
                fflonk_snark_wrapper_vk_hash
            FROM
                protocol_patches
            WHERE
                minor = $1
                AND patch = $2
            "#,
            version.minor as i32,
            version.patch.0 as i32
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()?;
        Some(L1VerifierConfig {
            snark_wrapper_vk_hash: H256::from_slice(&row.snark_wrapper_vk_hash),
            fflonk_snark_wrapper_vk_hash: row
                .fflonk_snark_wrapper_vk_hash
                .as_ref()
                .map(|x| H256::from_slice(x)),
        })
    }

    pub async fn get_patch_versions_for_vk(
        &mut self,
        minor_version: ProtocolVersionId,
        snark_wrapper_vk_hash: H256,
    ) -> DalResult<Vec<VersionPatch>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                patch
            FROM
                protocol_patches
            WHERE
                minor = $1
                AND snark_wrapper_vk_hash = $2
            ORDER BY
                patch DESC
            "#,
            minor_version as i32,
            snark_wrapper_vk_hash.as_bytes()
        )
        .instrument("get_patch_versions_for_vk")
        .fetch_all(self.storage)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| VersionPatch(row.patch as u32))
            .collect())
    }

    /// Returns first patch number for the minor version.
    /// Note, that some patch numbers can be skipped, so the result is not always 0.
    pub async fn first_patch_for_version(
        &mut self,
        version_id: ProtocolVersionId,
    ) -> DalResult<Option<VersionPatch>> {
        let row = sqlx::query!(
            r#"
            SELECT
                patch
            FROM
                protocol_patches
            WHERE
                minor = $1
            ORDER BY
                patch
            LIMIT
                1
            "#,
            version_id as i32,
        )
        .instrument("first_patch_for_version")
        .fetch_optional(self.storage)
        .await?;
        Ok(row.map(|row| VersionPatch(row.patch as u32)))
    }

    pub async fn latest_semantic_version(&mut self) -> DalResult<Option<ProtocolSemanticVersion>> {
        sqlx::query!(
            r#"
            SELECT
                minor,
                patch
            FROM
                protocol_patches
            ORDER BY
                minor DESC,
                patch DESC
            LIMIT
                1
            "#
        )
        .try_map(|row| {
            parse_protocol_version(row.minor).map(|minor| ProtocolSemanticVersion {
                minor,
                patch: (row.patch as u32).into(),
            })
        })
        .instrument("latest_semantic_version")
        .fetch_optional(self.storage)
        .await
    }

    pub async fn last_used_version_id(&mut self) -> Option<ProtocolVersionId> {
        let id = sqlx::query!(
            r#"
            SELECT
                protocol_version
            FROM
                l1_batches
            WHERE
                is_sealed
            ORDER BY
                number DESC
            LIMIT
                1
            "#
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()?
        .protocol_version?;

        Some((id as u16).try_into().unwrap())
    }

    pub async fn all_versions(&mut self) -> Vec<ProtocolSemanticVersion> {
        let rows = sqlx::query!(
            r#"
            SELECT
                minor,
                patch
            FROM
                protocol_patches
            "#
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap();
        rows.into_iter()
            .map(|row| ProtocolSemanticVersion {
                minor: (row.minor as u16).try_into().unwrap(),
                patch: (row.patch as u32).into(),
            })
            .collect()
    }

    pub async fn get_protocol_upgrade_tx(
        &mut self,
        protocol_version_id: ProtocolVersionId,
    ) -> DalResult<Option<ProtocolUpgradeTx>> {
        let instrumentation = Instrumented::new("get_protocol_upgrade_tx")
            .with_arg("protocol_version_id", &protocol_version_id);
        let query = sqlx::query!(
            r#"
            SELECT
                upgrade_tx_hash
            FROM
                protocol_versions
            WHERE
                id = $1
            "#,
            protocol_version_id as i32
        );

        let maybe_row = instrumentation
            .with(query)
            .fetch_optional(self.storage)
            .await?;
        let Some(upgrade_tx_hash) = maybe_row.and_then(|row| row.upgrade_tx_hash) else {
            return Ok(None);
        };
        let upgrade_tx_hash = H256::from_slice(&upgrade_tx_hash);

        let instrumentation = Instrumented::new("get_protocol_upgrade_tx#get_tx")
            .with_arg("protocol_version_id", &protocol_version_id)
            .with_arg("upgrade_tx_hash", &upgrade_tx_hash);
        let tx = self
            .storage
            .transactions_dal()
            .get_tx_by_hash(upgrade_tx_hash)
            .await?
            .ok_or_else(|| {
                instrumentation.arg_error(
                    "upgrade_tx_hash",
                    anyhow::anyhow!("upgrade transaction is not present in storage"),
                )
            })?;
        let tx = tx
            .try_into()
            .map_err(|err| instrumentation.arg_error("tx", anyhow::Error::msg(err)))?;
        Ok(Some(tx))
    }
}
