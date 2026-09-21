use crate::{
    versions::testonly::bootloader::{test_bootloader_out_of_gas, test_dummy_bootloader},
    vm_fast::Vm,
};

#[test]
fn dummy_bootloader() {
    test_dummy_bootloader::<Vm<_>>();
}

#[test]
fn bootloader_out_of_gas() {
    test_bootloader_out_of_gas::<Vm<_>>();
}

#[test]
fn bootloader_hook_near_call_can_access_root_heap() {
    use zksync_types::U256;
    use zksync_vm2::{
        interface::{CallframeInterface, StateInterface},
        ExecutionEnd,
    };

    use crate::{versions::testonly::VmTesterBuilder, vm_fast::tracers::WithBuiltinTracers};

    let mut tester = VmTesterBuilder::new().build::<Vm<_>>();
    let vm = &mut tester.vm;
    assert!(!vm.has_previous_far_calls());

    // Since zksolc 1.5.17 the production bootloader emits hooks from a
    // NoInline near-call helper. Exercise the real compiled bootloader.
    let result = vm.inner.run(&mut vm.world, &mut WithBuiltinTracers::mock());
    assert!(matches!(result, ExecutionEnd::SuspendedOnHook(_)));
    assert!(vm.inner.current_frame().is_near_call());
    assert!(!vm.has_previous_far_calls());

    let original_word = vm.read_word_from_bootloader_heap(0);
    let replacement = original_word + U256::one();
    vm.write_to_bootloader_heap([(0, replacement)]);
    assert_eq!(vm.read_word_from_bootloader_heap(0), replacement);
}

#[test]
#[should_panic(expected = "Cannot write to bootloader heap when not in root call frame")]
fn far_call_cannot_write_bootloader_heap() {
    use zksync_types::U256;
    use zksync_vm2::{
        interface::{GlobalStateInterface, Opcode, OpcodeType, ShouldStop, Tracer},
        ExecutionEnd,
    };

    use crate::{versions::testonly::VmTesterBuilder, vm_fast::tracers::WithBuiltinTracers};

    #[derive(Debug, Default)]
    struct StopAfterFarCall;

    impl Tracer for StopAfterFarCall {
        fn after_instruction<OP: OpcodeType, S: GlobalStateInterface>(
            &mut self,
            _state: &mut S,
        ) -> ShouldStop {
            if matches!(OP::VALUE, Opcode::FarCall(_)) {
                ShouldStop::Stop
            } else {
                ShouldStop::Continue
            }
        }
    }

    let mut tester = VmTesterBuilder::new().build::<Vm<_, StopAfterFarCall>>();
    let vm = &mut tester.vm;
    let mut tracer = WithBuiltinTracers::mock();
    // Execute genuine bootloader instructions until it calls a system contract.
    // Intermediate debug hooks do not require operator-supplied memory.
    loop {
        match vm.inner.run(&mut vm.world, &mut tracer) {
            ExecutionEnd::SuspendedOnHook(_) => continue,
            ExecutionEnd::StoppedByTracer => break,
            other => panic!("Expected bootloader far call, got {other:?}"),
        }
    }
    assert!(vm.has_previous_far_calls());
    vm.write_to_bootloader_heap([(0, U256::one())]);
}
