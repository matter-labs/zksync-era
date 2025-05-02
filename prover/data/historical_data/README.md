# Historical keys and hashes

This directory contains historical verification keys and hashes. The name of the subdirectory should match the protocol
version.

| Version | Description                          | Circuit version | Other                   |
| ------- | ------------------------------------ | --------------- | ----------------------- |
| 18      | boojum - 1.4.0                       | 1.4.0           |                         |
| 19      | boojum fix                           |                 |                         |
| 20      | fee model - 1.4.1                    | 1.4.1           |                         |
| 21      | blobs - 1.4.2                        | 1.4.2           |                         |
| 22      | fix - 1.4.2                          |                 |                         |
| 23      | 16 blobs + AA hashes + shared bridge | 1.5.0           |                         |
| 24.0    | 23 + fixes                           |                 |                         |
| 24.1    | fixes                                |                 |                         |
| 25.0    | protocol defence                     |                 | no circuit changes      |
| 26.0    | gateway & bridges                    |                 | no circuit changes      |
| 27.0    | EVM emulator & FFLONK                | 1.5.1           | compressor keys changed |
| 28.0    | gateway precompiles                  |                 | added 4 circuits        |

And from version 24, we switched to semver (so 0.24.0, 0.24.1 etc).
