use super::ActionQueue;

/// The task that keeps checking for the new batch status changes and persists them in the database.
pub fn run_batch_status_updater(actions: ActionQueue) {
    loop {
        let changes = actions.take_status_changes();
        for change in changes.commit {
            vlog::info!(
                "Commit status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );
        }
        for change in changes.prove {
            vlog::info!(
                "Prove status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );
        }
        for change in changes.execute {
            vlog::info!(
                "Execute status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );
        }
    }
}
