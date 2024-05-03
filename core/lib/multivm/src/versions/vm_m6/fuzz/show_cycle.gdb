set pagination off

define show_cycle
    break cycle.rs:395 if self.local_state.monotonic_cycle_counter == $arg0
    run
    p self.local_state.registers
    quit
end
