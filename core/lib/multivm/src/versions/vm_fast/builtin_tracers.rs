use zksync_vm2::interface::Tracer;
use zksync_vm_interface::tracer::ValidationParams;

use super::{
    circuits_tracer::CircuitsTracer,
    evm_deploy_tracer::{DynamicBytecodes, EvmDeployTracer},
    validation_tracer::{FullValidationTracer, ValidationGasLimitOnly, ValidationTracer},
};

#[derive(Debug, Default)]
pub struct WithBuiltinTracers<External, Validation>(
    (
        External,
        (Validation, (CircuitsTracer, (EvmDeployTracer, ()))),
    ),
);

pub type DefaultTracers = SequencerTracers<()>;

pub type ValidationTracers<Tr> = WithBuiltinTracers<Tr, FullValidationTracer>;

impl<External> ValidationTracers<External> {
    pub fn for_validation(
        external: External,
        validation_params: ValidationParams,
        timestamp: u64,
    ) -> Self {
        Self((
            external,
            (
                FullValidationTracer::new(validation_params, timestamp),
                Default::default(),
            ),
        ))
    }
}

pub type ApiTracers<Tr> = WithBuiltinTracers<Tr, ValidationGasLimitOnly>;

impl<External> ApiTracers<External> {
    pub fn for_api(external: External) -> Self {
        Self((external, Default::default()))
    }
}

pub type SequencerTracers<Tr> = WithBuiltinTracers<Tr, ValidationGasLimitOnly>;

impl<External> SequencerTracers<External> {
    pub fn for_sequencer(external: External) -> Self {
        Self((external, Default::default()))
    }
}

impl<External, Validation> WithBuiltinTracers<External, Validation> {
    pub fn into_inner(self) -> External {
        self.0 .0
    }

    pub fn validation(&mut self) -> &mut Validation {
        self.0.get()
    }

    pub(super) fn circuit(&mut self) -> &mut CircuitsTracer {
        self.0.get()
    }

    pub(super) fn insert_dynamic_bytecodes_handle(&mut self, dynamic_bytecodes: DynamicBytecodes) {
        let deploy_tracer: &mut EvmDeployTracer = self.0.get();
        deploy_tracer.insert_dynamic_bytecodes_handle(dynamic_bytecodes)
    }
}

impl<External: Tracer, Validation: ValidationTracer> Tracer
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

// Heteregeneus list item access

struct Here;
struct Later<T>(T);
trait ListGet<T, W> {
    fn get(&mut self) -> &mut T;
}

impl<T, U> ListGet<T, Here> for (T, U) {
    fn get(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T, W, U, V: ListGet<T, W>> ListGet<T, Later<W>> for (U, V) {
    fn get(&mut self) -> &mut T {
        self.1.get()
    }
}
