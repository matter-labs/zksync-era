/// Execution mode determines when the virtual machine execution should stop.
/// We are also using a different set of tracers, depending on the selected mode - for example for OneTx,
/// we use Refund Tracer, and for Bootloader we use 'DefaultTracer` in a special mode to track the Bootloader return code
/// Flow of execution:
/// VmStarted -> Enter the bootloader -> Tx1 -> Tx2 -> ... -> TxN ->
/// -> Terminate bootloader execution -> Exit bootloader -> VmStopped
#[derive(Debug, Copy, Clone)]
pub enum VmExecutionMode {
    /// Stop after executing the next transaction.
    OneTx,
    /// Stop after executing the entire batch.
    Batch,
    /// Stop after executing the entire bootloader. But before you exit the bootloader.
    Bootloader,
}
