use zksync_vm2::interface::Tracer;
use zksync_vm_interface::tracer::ValidationParams;

use super::{
    circuits_tracer::CircuitsTracer,
    evm_deploy_tracer::{DynamicBytecodes, EvmDeployTracer},
    validation_tracer::{ValidationGasLimitOnly, ValidationMode, ValidationTracer},
};

#[derive(Debug, Default)]
pub struct WithBuiltinTracers<External, Validation>(
    (External, (Validation, (CircuitsTracer, EvmDeployTracer))),
);

pub type DefaultTracers = WithBuiltinTracersForSequencer<()>;

pub type WithBuiltinTracersForValidation<Tr> = WithBuiltinTracers<Tr, ValidationTracer>;

impl<External> WithBuiltinTracersForValidation<External> {
    pub fn for_validation(
        external: External,
        validation_params: ValidationParams,
        timestamp: u64,
    ) -> Self {
        Self((
            external,
            (
                ValidationTracer::new(validation_params, timestamp),
                Default::default(),
            ),
        ))
    }
}

pub type WithBuiltinTracersForApi<Tr> = WithBuiltinTracers<Tr, ValidationGasLimitOnly>;

impl<External> WithBuiltinTracersForApi<External> {
    pub fn for_api(external: External) -> Self {
        Self((external, Default::default()))
    }
}

pub type WithBuiltinTracersForSequencer<Tr> = WithBuiltinTracers<Tr, ValidationGasLimitOnly>;

impl<External> WithBuiltinTracersForSequencer<External> {
    pub fn for_sequencer(external: External) -> Self {
        Self((external, Default::default()))
    }
}

impl<External, Validation> WithBuiltinTracers<External, Validation> {
    pub fn into_inner(self) -> External {
        self.0 .0
    }

    pub fn validation(&mut self) -> &mut Validation {
        &mut self.0 .1 .0
    }

    pub(super) fn circuit(&mut self) -> &mut CircuitsTracer {
        &mut self.0 .1 .1 .0
    }

    pub(super) fn insert_dynamic_bytecodes_handle(&mut self, dynamic_bytecodes: DynamicBytecodes) {
        self.0
             .1
             .1
             .1
            .insert_dynamic_bytecodes_handle(dynamic_bytecodes)
    }
}

impl<External: Tracer, Validation: ValidationMode> Tracer
    for WithBuiltinTracers<External, Validation>
{
    fn before_instruction<
        OP: zksync_vm2::interface::OpcodeType,
        S: zksync_vm2::interface::GlobalStateInterface,
    >(
        &mut self,
        state: &mut S,
    ) {
        self.0.before_instruction::<OP, _>(state)
    }

    fn after_instruction<
        OP: zksync_vm2::interface::OpcodeType,
        S: zksync_vm2::interface::GlobalStateInterface,
    >(
        &mut self,
        state: &mut S,
    ) -> zksync_vm2::interface::ShouldStop {
        self.0.after_instruction::<OP, _>(state)
    }

    fn on_extra_prover_cycles(&mut self, stats: zksync_vm2::interface::CycleStats) {
        self.0.on_extra_prover_cycles(stats);
    }
}
