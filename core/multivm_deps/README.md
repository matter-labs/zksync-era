# MultiVM dependencies

This folder contains the old versions of the VM we have used in the past. The `multivm` crate uses them to dynamically
switch the version we use to be able to sync from the genesis. This is a temporary measure until a "native" solution is
implemented (i.e., the `vm` crate would itself know the changes between versions, and thus we will have only the
functional diff between versions, not several fully-fledged VMs).
