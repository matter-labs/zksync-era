use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::PriorityOpId;

use crate::Core;

/// Operator-inserted force-fail marks for priority operations. The sequencer applies them when
/// handing an L1->L2 tx to the VM; `transactions.force_fail` records what actually executed.
#[derive(Debug)]
pub struct ForceFailDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl ForceFailDal<'_, '_> {
    pub async fn insert_force_failed_priority_op(
        &mut self,
        priority_op_id: PriorityOpId,
        reason: Option<&str>,
        created_by: &str,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
            force_failed_priority_ops (priority_op_id, reason, created_by)
            VALUES
            ($1, $2, $3)
            ON CONFLICT (priority_op_id) DO NOTHING
            "#,
            priority_op_id.0 as i64,
            reason,
            created_by,
        )
        .instrument("insert_force_failed_priority_op")
        .with_arg("priority_op_id", &priority_op_id)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// Returns every mark with its operator-supplied reason. The table holds a handful of rows, so
    /// there is no point in bounding the query.
    pub async fn get_force_failed_priority_ops(
        &mut self,
    ) -> DalResult<Vec<(PriorityOpId, Option<String>)>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                priority_op_id,
                reason
            FROM
                force_failed_priority_ops
            ORDER BY
                priority_op_id
            "#
        )
        .instrument("get_force_failed_priority_ops")
        .fetch_all(self.storage)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (PriorityOpId(row.priority_op_id as u64), row.reason))
            .collect())
    }
}
