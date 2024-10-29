pub use zksync_vm2::interface;

pub use self::{circuits_tracer::CircuitsTracer, vm::Vm};

mod bootloader_state;
mod bytecode;
mod circuits_tracer;
mod events;
mod glue;
mod hook;
mod initial_bootloader_memory;
mod pubdata;
mod refund;
// FIXME(EVM-711): restore tests for fast VM once it is integrated
#[cfg(test)]
mod tests;
mod transaction_data;
mod vm;
