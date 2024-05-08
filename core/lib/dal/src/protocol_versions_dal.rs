use std::convert::TryInto;

use anyhow::Context as _;
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes};
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
};
use zksync_types::{
    protocol_upgrade::{ProtocolUpgradeTx, ProtocolVersion},
    protocol_version::{L1VerifierConfig, VerifierParams},
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
        id: ProtocolVersionId,
        timestamp: u64,
        l1_verifier_config: L1VerifierConfig,
        base_system_contracts_hashes: BaseSystemContractsHashes,
        tx_hash: Option<H256>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                protocol_versions (
                    id,
                    timestamp,
                    recursion_scheduler_level_vk_hash,
                    recursion_node_level_vk_hash,
                    recursion_leaf_level_vk_hash,
                    recursion_circuits_set_vks_hash,
                    bootloader_code_hash,
                    default_account_code_hash,
                    evm_simulator_code_hash,
                    upgrade_tx_hash,
                    created_at
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
            "#,
            id as i32,
            timestamp as i64,
            l1_verifier_config
                .recursion_scheduler_level_vk_hash
                .as_bytes(),
            l1_verifier_config
                .params
                .recursion_node_level_vk_hash
                .as_bytes(),
            l1_verifier_config
                .params
                .recursion_leaf_level_vk_hash
                .as_bytes(),
            l1_verifier_config
                .params
                .recursion_circuits_set_vks_hash
                .as_bytes(),
            base_system_contracts_hashes.bootloader.as_bytes(),
            base_system_contracts_hashes.default_aa.as_bytes(),
            base_system_contracts_hashes.evm_simulator.as_bytes(),
            tx_hash.as_ref().map(H256::as_bytes),
        )
        .instrument("save_protocol_version")
        .with_arg("id", &id)
        .with_arg(
            "base_system_contracts_hashes",
            &base_system_contracts_hashes,
        )
        .with_arg("tx_hash", &tx_hash)
        .execute(self.storage)
        .await?;
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
                version.id,
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
                bootloader_code_hash,
                default_account_code_hash,
                evm_simulator_code_hash,
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

    pub async fn load_base_system_contracts_by_version_id(
        &mut self,
        version_id: u16,
    ) -> anyhow::Result<Option<BaseSystemContracts>> {
        let row = sqlx::query!(
            r#"
            SELECT
                bootloader_code_hash,
                default_account_code_hash,
                evm_simulator_code_hash
            FROM
                protocol_versions
            WHERE
                id = $1
            "#,
            i32::from(version_id)
        )
        .fetch_optional(self.storage.conn())
        .await
        .context("cannot fetch system contract hashes")?;

        Ok(if let Some(row) = row {
            let contracts = self
                .storage
                .factory_deps_dal()
                .get_base_system_contracts(
                    H256::from_slice(&row.bootloader_code_hash),
                    H256::from_slice(&row.default_account_code_hash),
                    row.evm_simulator_code_hash
                        .map(|hash| H256::from_slice(&hash)),
                )
                .await?;

            Some(contracts)
        } else {
            None
        })
    }

    pub async fn load_previous_version(
        &mut self,
        version_id: ProtocolVersionId,
    ) -> DalResult<Option<ProtocolVersion>> {
        let maybe_version = sqlx::query_as!(
            StorageProtocolVersion,
            r#"
            SELECT
                *
            FROM
                protocol_versions
            WHERE
                id < $1
            ORDER BY
                id DESC
            LIMIT
                1
            "#,
            version_id as i32
        )
        .try_map(|row| Ok((parse_protocol_version(row.id)?, row)))
        .instrument("load_previous_version")
        .with_arg("version_id", &version_id)
        .fetch_optional(self.storage)
        .await?;

        let Some((version_id, row)) = maybe_version else {
            return Ok(None);
        };
        let tx = self.get_protocol_upgrade_tx(version_id).await?;
        Ok(Some(protocol_version_from_storage(row, tx)))
    }

    pub async fn get_protocol_version(
        &mut self,
        version_id: ProtocolVersionId,
    ) -> DalResult<Option<ProtocolVersion>> {
        let maybe_row = sqlx::query_as!(
            StorageProtocolVersion,
            r#"
            SELECT
                *
            FROM
                protocol_versions
            WHERE
                id = $1
            "#,
            version_id as i32
        )
        .instrument("get_protocol_version")
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
        version_id: ProtocolVersionId,
    ) -> Option<L1VerifierConfig> {
        let row = sqlx::query!(
            r#"
            SELECT
                recursion_scheduler_level_vk_hash,
                recursion_node_level_vk_hash,
                recursion_leaf_level_vk_hash,
                recursion_circuits_set_vks_hash
            FROM
                protocol_versions
            WHERE
                id = $1
            "#,
            version_id as i32
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()?;
        Some(L1VerifierConfig {
            params: VerifierParams {
                recursion_node_level_vk_hash: H256::from_slice(&row.recursion_node_level_vk_hash),
                recursion_leaf_level_vk_hash: H256::from_slice(&row.recursion_leaf_level_vk_hash),
                recursion_circuits_set_vks_hash: H256::from_slice(
                    &row.recursion_circuits_set_vks_hash,
                ),
            },
            recursion_scheduler_level_vk_hash: H256::from_slice(
                &row.recursion_scheduler_level_vk_hash,
            ),
        })
    }

    pub async fn last_version_id(&mut self) -> DalResult<Option<ProtocolVersionId>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                MAX(id) AS "max?"
            FROM
                protocol_versions
            "#
        )
        .try_map(|row| row.max.map(parse_protocol_version).transpose())
        .instrument("last_version_id")
        .fetch_optional(self.storage)
        .await?
        .flatten())
    }

    pub async fn last_used_version_id(&mut self) -> Option<ProtocolVersionId> {
        let id = sqlx::query!(
            r#"
            SELECT
                protocol_version
            FROM
                l1_batches
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

    pub async fn all_version_ids(&mut self) -> Vec<ProtocolVersionId> {
        let rows = sqlx::query!(
            r#"
            SELECT
                id
            FROM
                protocol_versions
            "#
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap();
        rows.into_iter()
            .map(|row| (row.id as u16).try_into().unwrap())
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
