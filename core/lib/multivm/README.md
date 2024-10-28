# ZKsync Era VM Wrapper

This crate represents a wrapper over several versions of VM that have been used by the ZKsync Era node. It contains the
glue code that allows switching the VM version based on the externally provided marker while preserving the public
interface. This crate exists to enable the external node to process breaking upgrades and re-execute all the
transactions from the genesis block.

## Developer guidelines

### Adding tests

If you want to add unit tests for the VM wrapper, consider the following:

- Whenever possible, make tests reusable; declare test logic in the [`testonly`](src/versions/testonly/mod.rs) module,
  and then instantiate tests using this logic for the supported VM versions. If necessary, extend the tested VM trait so
  that test logic can be defined in a generic way. See the `testonly` module docs for more detailed guidelines.
- If you define a generic test, don't forget to add its instantiations for all supported VMs (`vm_latest`, `vm_fast` and
  `shadow`). `shadow` tests allow checking VM divergences for free!
- Do not use an RNG where it can be avoided (e.g., for test contract addresses).
- Avoid using zero / default values in cases they can be treated specially by the tested code.
