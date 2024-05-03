# MultiVM dependencies

This folder contains the old versions of the VM we have used in the past. The `multivm` crate uses them to dynamically
switch the version we use to be able to sync from the genesis. This is a temporary measure until a "native" solution is
implemented (i.e., the `vm` crate would itself know the changes between versions, and thus we will have only the
functional diff between versions, not several fully-fledged VMs).

## Versions

| Name                   | Protocol versions | Description                                                           |
| ---------------------- | ----------------- | --------------------------------------------------------------------- |
| vm_m5                  | 0 - 3             | Release for the testnet launch                                        |
| vm_m6                  | 4 - 6             | Release for the mainnet launch                                        |
| vm_1_3_2               | 7 - 12            | Release 1.3.2 of the crypto circuits                                  |
| vm_virtual_blocks      | 13 - 15           | Adding virtual blocks to help with block number / timestamp migration |
| vm_refunds_enhancement | 16 - 17           | Fixing issue related to refunds in VM                                 |
| vm_boojum_integration  | 18 -              | New Proving system (boojum), vm version 1.4.0                         |
