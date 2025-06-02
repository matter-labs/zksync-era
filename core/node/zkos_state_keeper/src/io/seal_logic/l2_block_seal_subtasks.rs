use std::collections::HashMap;

use anyhow::Context;
use async_trait::async_trait;
use zk_os_basic_system::system_implementation::flat_storage_model::ACCOUNT_PROPERTIES_STORAGE_ADDRESS;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{h256_to_address, L2BlockNumber};

use crate::updates::BlockSealCommand;

/// Helper struct that encapsulates parallel l2 block sealing logic.
#[derive(Debug)]
pub struct L2BlockSealProcess;

impl L2BlockSealProcess {
    pub fn all_subtasks() -> Vec<Box<dyn L2BlockSealSubtask>> {
        vec![
            Box::new(MarkTransactionsInL2BlockSubtask),
            Box::new(InsertStorageLogsSubtask),
            Box::new(InsertFactoryDepsSubtask),
            Box::new(InsertEventsSubtask),
            Box::new(InsertL2ToL1LogsSubtask),
            Box::new(InsertAccountPropertiesSubtask),
        ]
    }

    pub fn subtasks_len() -> u32 {
        Self::all_subtasks().len() as u32
    }

    pub async fn run_subtasks(
        command: &BlockSealCommand,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let subtasks = Self::all_subtasks();

        let handles = subtasks.into_iter().map(|subtask| {
            let subtask_name = subtask.name();
            async move {
                let mut connection = pool.connection_tagged("state_keeper").await?;
                subtask
                    .run(command, &mut connection)
                    .await
                    .context(subtask_name)
            }
        });
        futures::future::try_join_all(handles).await?;

        Ok(())
    }

    /// Clears pending l2 block data from the database.
    pub async fn clear_pending_l2_block(
        connection: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        let seal_subtasks = L2BlockSealProcess::all_subtasks();
        for subtask in seal_subtasks {
            subtask.rollback(connection, last_sealed_l2_block).await?;
        }

        Ok(())
    }
}

/// An abstraction that represents l2 block seal sub-task that can be run in parallel with other sub-tasks.
#[async_trait::async_trait]
pub trait L2BlockSealSubtask: Send + Sync + 'static {
    /// Returns sub-task name.
    fn name(&self) -> &'static str;

    /// Runs seal process.
    async fn run(
        self: Box<Self>,
        command: &BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()>;

    /// Rollbacks data that was saved to database for the pending L2 block.
    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()>;
}

#[derive(Debug)]
pub(super) struct MarkTransactionsInL2BlockSubtask;

#[async_trait]
impl L2BlockSealSubtask for MarkTransactionsInL2BlockSubtask {
    fn name(&self) -> &'static str {
        "mark_transactions_in_l2_block"
    }

    async fn run(
        self: Box<Self>,
        command: &BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        connection
            .transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                command.inner.l2_block_number,
                &command.inner.executed_transactions,
                command.inner.base_fee_per_gas.into(),
                command.inner.protocol_version,
                command.pre_insert_txs,
            )
            .await?;

        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .transactions_dal()
            .reset_transactions_state(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct InsertStorageLogsSubtask;

#[async_trait]
impl L2BlockSealSubtask for InsertStorageLogsSubtask {
    fn name(&self) -> &'static str {
        "insert_storage_logs"
    }

    async fn run(
        self: Box<Self>,
        command: &BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        let write_logs = command.extract_deduplicated_write_logs();

        connection
            .storage_logs_dal()
            .insert_storage_logs(command.inner.l2_block_number, &write_logs)
            .await?;

        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .storage_logs_dal()
            .roll_back_storage_logs(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct InsertFactoryDepsSubtask;

#[async_trait]
impl L2BlockSealSubtask for InsertFactoryDepsSubtask {
    fn name(&self) -> &'static str {
        "insert_factory_deps"
    }

    async fn run(
        self: Box<Self>,
        command: &BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        if !command.inner.new_factory_deps.is_empty() {
            connection
                .factory_deps_dal()
                .insert_factory_deps(
                    command.inner.l2_block_number,
                    &command.inner.new_factory_deps,
                )
                .await?;
        }

        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .factory_deps_dal()
            .roll_back_factory_deps(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct InsertEventsSubtask;

#[async_trait]
impl L2BlockSealSubtask for InsertEventsSubtask {
    fn name(&self) -> &'static str {
        "insert_events"
    }

    async fn run(
        self: Box<Self>,
        command: &BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        let l2_block_events = command.extract_events();
        let l2_block_event_count: usize =
            l2_block_events.iter().map(|(_, events)| events.len()).sum();

        connection
            .events_dal()
            .save_events(command.inner.l2_block_number, &l2_block_events)
            .await?;
        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .events_dal()
            .roll_back_events(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct InsertL2ToL1LogsSubtask;

#[async_trait]
impl L2BlockSealSubtask for InsertL2ToL1LogsSubtask {
    fn name(&self) -> &'static str {
        "insert_l2_to_l1_logs"
    }

    async fn run(
        self: Box<Self>,
        command: &BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        //todo: l2_to_l1_logs are not saved currently
        let user_l2_to_l1_logs = command.extract_user_l2_to_l1_logs();
        let user_l2_to_l1_log_count: usize = user_l2_to_l1_logs
            .iter()
            .map(|(_, l2_to_l1_logs)| l2_to_l1_logs.len())
            .sum();

        // if !user_l2_to_l1_logs.is_empty() {
        //     connection
        //         .events_dal()
        //         .save_user_l2_to_l1_logs(command.inner.l2_block_number, &user_l2_to_l1_logs)
        //         .await?;
        // }
        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .events_dal()
            .roll_back_l2_to_l1_logs(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct InsertAccountPropertiesSubtask;

#[async_trait]
impl L2BlockSealSubtask for InsertAccountPropertiesSubtask {
    fn name(&self) -> &'static str {
        "insert_account_properties"
    }

    async fn run(
        self: Box<Self>,
        command: &BlockSealCommand,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        let mut new_properties: HashMap<_, _> =
            command.inner.new_account_data.clone().into_iter().collect();
        let mut to_insert = Vec::new();
        for log in &command.inner.storage_logs {
            if log.key.address().0 == ACCOUNT_PROPERTIES_STORAGE_ADDRESS.to_be_bytes() {
                if let Some(properties) = new_properties.remove(&log.value) {
                    let account_address = h256_to_address(log.key.key());
                    to_insert.push((account_address, properties));
                }
            }
        }

        if !new_properties.is_empty() {
            anyhow::bail!("could not map account properties to addresses");
        }

        if !to_insert.is_empty() {
            connection
                .account_properies_dal()
                .insert_account_properties(command.inner.l2_block_number, &to_insert)
                .await?;
        }

        Ok(())
    }

    async fn rollback(
        &self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        storage
            .account_properies_dal()
            .roll_back_properties(last_sealed_l2_block)
            .await?;
        Ok(())
    }
}
