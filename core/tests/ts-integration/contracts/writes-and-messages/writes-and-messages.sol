// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;
pragma abicoder v2;

interface IL2Messenger {
    function sendToL1(bytes memory _message) external returns (bytes32);
}

contract WritesAndMessages {
    IL2Messenger constant L2_TO_L1_MESSENGER = IL2Messenger(address(0x8008));
    mapping(uint256 => uint256) public s;

    function writes(uint from, uint iterations, uint value) public {
        unchecked {
            for (uint i = 0; i < iterations; i++) {
                assembly {
                    sstore(add(from, i), value)
                }
            }
        }
    }

    function l2_l1_messages(uint iterations) public {
        unchecked {
            for (uint i = 0; i < iterations; i++) {
                L2_TO_L1_MESSENGER.sendToL1(abi.encode(i));
            }
        }
    }

    function big_l2_l1_message(uint size) public {
        unchecked {
            bytes memory message = new bytes(size);
            for (uint i = 0; i < size; i++) {
                message[i] = bytes1(uint8(i % 256));
            }
            L2_TO_L1_MESSENGER.sendToL1(message);
        }
    }
}
