# ZKsync Era VM Wrapper

This crate represents a wrapper over several versions of VM that have been used by the ZKsync Era node. It contains the
glue code that allows switching the VM version based on the externally provided marker while preserving the public
interface. This crate exists to enable the external node to process breaking upgrades and re-execute all the
transactions from the genesis block.
