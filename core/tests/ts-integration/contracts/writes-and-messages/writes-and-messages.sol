// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;
pragma abicoder v2;

interface IL2Messenger {
    function sendToL1(bytes memory _message) external returns (bytes32);
}

contract WritesAndMessages {
    IL2Messenger constant L2_MESSENGER = IL2Messenger(address(0x8008));
    mapping(uint256 => uint256) public s;

    uint256 constant LOOP_LIMIT = 1000; // Adjust this value based on your gas limit and requirements

    function writes(uint from, uint iterations, uint value) public {
        unchecked {
            uint256 remaining = iterations;
            uint256 start = from;
            while (remaining > 0) {
                uint256 batchSize = remaining > LOOP_LIMIT ? LOOP_LIMIT : remaining;
                for (uint256 i = 0; i < batchSize; i++) {
                    assembly {
                        sstore(add(start, i), value)
                    }
                }
                remaining -= batchSize;
                start += batchSize;
            }
        }
    }

    function l2_l1_messages(uint iterations) public {
        unchecked {
            for (uint i = 0; i < iterations; i++) {
                L2_MESSENGER.sendToL1(abi.encode(i));
            }
        }
    }

    function big_l2_l1_message(uint size) public {
        unchecked {
            bytes memory message = new bytes(size);
            for (uint i = 0; i < size; i++) {
                message[i] = bytes1(uint8(i % 256));
            }
            L2_MESSENGER.sendToL1(message);
        }
    }
}