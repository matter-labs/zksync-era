//! Tracers for the fast VM.

use zksync_vm2::interface::{CycleStats, GlobalStateInterface, OpcodeType, ShouldStop, Tracer};

pub(super) use self::evm_deploy::DynamicBytecodes;
pub use self::{
    calls::CallTracer,
    storage::StorageInvocationsTracer,
    validation::{FastValidationTracer, FullValidationTracer, ValidationTracer},
};
use self::{circuits::CircuitsTracer, evm_deploy::EvmDeployTracer};
use crate::interface::CircuitStatistic;

mod calls;
mod circuits;
mod evm_deploy;
mod storage;
mod validation;

#[derive(Debug)]
pub(super) struct WithBuiltinTracers<Ext, Val> {
    pub external: Ext,
    pub validation: Val,
    circuits: CircuitsTracer,
    evm_deploy_tracer: EvmDeployTracer,
}

impl<Tr: Tracer, Val: ValidationTracer> WithBuiltinTracers<Tr, Val> {
    pub(super) fn new(external: Tr, validation: Val, dynamic_bytecodes: DynamicBytecodes) -> Self {
        Self {
            external,
            validation,
            circuits: CircuitsTracer::default(),
            evm_deploy_tracer: EvmDeployTracer::new(dynamic_bytecodes),
        }
    }

    pub(super) fn circuit_statistic(&self) -> CircuitStatistic {
        self.circuits.circuit_statistic()
    }
}

#[cfg(test)]
impl<Tr: Tracer + Default, Val: ValidationTracer> WithBuiltinTracers<Tr, Val> {
    pub(super) fn mock() -> Self {
        Self::new(Tr::default(), Val::default(), DynamicBytecodes::default())
    }
}

impl<Tr: Tracer, Val: Tracer> Tracer for WithBuiltinTracers<Tr, Val> {
    #[inline(always)]
    fn before_instruction<OP: OpcodeType, S: GlobalStateInterface>(&mut self, state: &mut S) {
        self.validation.before_instruction::<OP, _>(state);
        self.external.before_instruction::<OP, _>(state);
        self.circuits.before_instruction::<OP, _>(state);
        self.evm_deploy_tracer.before_instruction::<OP, _>(state);
    }

    #[inline(always)]
    fn after_instruction<OP: OpcodeType, S: GlobalStateInterface>(
        &mut self,
        state: &mut S,
    ) -> ShouldStop {
        if matches!(
            self.validation.after_instruction::<OP, _>(state),
            ShouldStop::Stop
        ) {
            return ShouldStop::Stop;
        }
        if matches!(
            self.external.after_instruction::<OP, _>(state),
            ShouldStop::Stop
        ) {
            return ShouldStop::Stop;
        }
        if matches!(
            self.circuits.after_instruction::<OP, _>(state),
            ShouldStop::Stop
        ) {
            return ShouldStop::Stop;
        }
        self.evm_deploy_tracer.after_instruction::<OP, _>(state)
    }

    #[inline(always)]
    fn on_extra_prover_cycles(&mut self, stats: CycleStats) {
        self.validation.on_extra_prover_cycles(stats);
        self.external.on_extra_prover_cycles(stats);
        self.circuits.on_extra_prover_cycles(stats);
        self.evm_deploy_tracer.on_extra_prover_cycles(stats);
    }
}
