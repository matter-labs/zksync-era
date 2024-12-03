use zksync_vm2::interface::{CycleStats, GlobalStateInterface, OpcodeType, ShouldStop, Tracer};

use super::{
    circuits_tracer::CircuitsTracer,
    evm_deploy_tracer::{DynamicBytecodes, EvmDeployTracer},
    validation_tracer::ValidationTracer,
};

#[derive(Debug)]
pub(super) struct WithBuiltinTracers<Ext, Val> {
    pub external: Ext,
    pub validation: Val,
    pub circuits: CircuitsTracer,
    pub evm_deploy_tracer: EvmDeployTracer,
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
