# Tool to better understand and debug provers

For now, it has only one command 'file-info'

```
cargo run --release  file-info /zksync-era/prover/artifacts/proofs_fri/l1_batch_proof_1.bin
```

Example outputs:

```
L1 proof
AUX info:
  L1 msg linear hash: [163, 243, 172, 16, 189, 59, 100, 227, 249, 46, 226, 220, 82, 135, 213, 208, 221, 228, 49, 46, 121, 136, 78, 163, 15, 155, 199, 82, 64, 24, 172, 198]
  Rollup_state_diff_for_compression: [157, 150, 29, 193, 105, 162, 176, 61, 83, 241, 72, 206, 68, 20, 143, 69, 119, 162, 138, 101, 80, 139, 193, 211, 188, 250, 156, 86, 254, 148, 117, 60]
  bootloader_heap_initial_content: [112, 2, 120, 255, 156, 227, 172, 92, 134, 48, 247, 243, 148, 241, 11, 122, 6, 189, 46, 164, 89, 78, 209, 118, 115, 239, 195, 15, 225, 143, 97, 204]
  events_queue_state: [202, 78, 244, 233, 150, 17, 247, 25, 183, 51, 245, 110, 135, 31, 115, 109, 84, 193, 17, 1, 153, 32, 39, 199, 102, 25, 63, 216, 220, 68, 212, 233]
Inputs: [Fr(0x00000000775db828700e0ebbe0384f8a017598a271dfb6c96ebb2baf22a7a572)]
```

```
 == Circuit ==
Type: basic. Id: 1 (Scheduler)
Geometry: CSGeometry { num_columns_under_copy_permutation: 130, num_witness_columns: 0, num_constant_columns: 4, max_allowed_constraint_degree: 8 }
Circuit size: trace length: Some(1048576) something??: Some(100663296)
Scheduler witness info
Previous block data:
Enumeration counter: 25
State root: [107, 233, 138, 154, 21, 134, 189, 220, 183, 250, 117, 243, 103, 124, 71, 221, 160, 136, 249, 25, 197, 109, 8, 75, 26, 12, 81, 109, 36, 56, 30, 17]
Block meta parameters
bootloader code hash: 452367551810219221093730953379759186922674186246309239546886848509599206765
aa code hash: 452349823575802367618424269668644286404749728714566974110193150711820505769
Previous block meta hash: [63, 236, 0, 236, 23, 236, 175, 242, 75, 187, 203, 193, 88, 80, 202, 53, 40, 206, 28, 40, 125, 58, 53, 254, 233, 122, 108, 101, 101, 88, 102, 193]
Previous block aux hash: [200, 12, 70, 33, 103, 13, 251, 174, 96, 165, 135, 138, 34, 75, 249, 81, 93, 86, 110, 52, 30, 172, 198, 51, 155, 82, 86, 137, 156, 215, 11, 119]
EIP 4844 - witnesses: None
EIP 4844 - proofs: 0
```
