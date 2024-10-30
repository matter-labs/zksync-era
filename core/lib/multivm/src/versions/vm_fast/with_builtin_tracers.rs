use zksync_vm2::interface::Tracer;
use zksync_vm_interface::tracer::{ValidationParams, ViolatedValidationRule};

use super::{
    validation_tracer::{ValidationGasLimitOnly, ValidationMode, ValidationTracer},
    CircuitsTracer,
};

#[derive(Default, Debug)]
pub struct WithBuiltinTracers<External, Validation>((External, (CircuitsTracer, Validation)));

pub type DefaultTracers = WithBuiltinTracersForSequencer<()>;

pub type WithBuiltinTracersForValidation<Tr> = WithBuiltinTracers<Tr, ValidationTracer>;

impl<External> WithBuiltinTracersForValidation<External> {
    pub fn for_validation(external: External, validation_params: ValidationParams) -> Self {
        Self((
            external,
            (
                CircuitsTracer::default(),
                ValidationTracer::new(validation_params),
            ),
        ))
    }

    pub fn validation_error(&self) -> Option<ViolatedValidationRule> {
        self.0 .1 .1.validation_error()
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
        &mut self.0 .1 .1
    }

    pub fn circuit(&mut self) -> &mut CircuitsTracer {
        &mut self.0 .1 .0
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
    ) -> zksync_vm2::interface::ExecutionStatus {
        self.0.after_instruction::<OP, _>(state)
    }

    fn on_extra_prover_cycles(&mut self, stats: zksync_vm2::interface::CycleStats) {
        self.0.on_extra_prover_cycles(stats);
    }
}
