use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::L1BatchNumber;

use crate::Core;

/// Predicted vs. real Airbender guest cycle counts per L1 batch.
///
/// The predicted count is written by the state keeper when a batch is sealed (main node
/// only); the real count is written by the Airbender proof data handler when the prover
/// reports it alongside the FRI proof.
#[derive(Debug)]
pub struct CycleStatsDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

/// Cycle statistics for a single L1 batch. Either count may be missing: the prediction
/// is only persisted by the main node, and the real count only arrives with a proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L1BatchCycleStats {
    pub predicted_cycles: Option<u64>,
    pub real_cycles: Option<u64>,
}

impl CycleStatsDal<'_, '_> {
    pub async fn save_predicted_cycles(
        &mut self,
        l1_batch_number: L1BatchNumber,
        predicted_cycles: u64,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
            l1_batch_cycle_stats (l1_batch_number, predicted_cycles, created_at, updated_at)
            VALUES
            ($1, $2, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET
            predicted_cycles = $2,
            updated_at = NOW()
            "#,
            i64::from(l1_batch_number.0),
            predicted_cycles as i64,
        )
        .instrument("save_predicted_cycles")
        .with_arg("l1_batch_number", &l1_batch_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn save_real_cycles(
        &mut self,
        l1_batch_number: L1BatchNumber,
        real_cycles: u64,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
            l1_batch_cycle_stats (l1_batch_number, real_cycles, created_at, updated_at)
            VALUES
            ($1, $2, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET
            real_cycles = $2,
            updated_at = NOW()
            "#,
            i64::from(l1_batch_number.0),
            real_cycles as i64,
        )
        .instrument("save_real_cycles")
        .with_arg("l1_batch_number", &l1_batch_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn get_cycle_stats(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<L1BatchCycleStats>> {
        let row = sqlx::query!(
            r#"
            SELECT
                predicted_cycles,
                real_cycles
            FROM
                l1_batch_cycle_stats
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0),
        )
        .instrument("get_cycle_stats")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| L1BatchCycleStats {
            predicted_cycles: row.predicted_cycles.map(|cycles| cycles as u64),
            real_cycles: row.real_cycles.map(|cycles| cycles as u64),
        }))
    }
}
