// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

contract FailedCall {
    bool public success;
    bytes1 public data_first_byte;
    
    constructor() {
        address MSG_VALUE_SIMULATOR = 0x0000000000000000000000000000000000008009;

		while (gasleft() > 20000) {
            // Burn gas so that there's about `gasToPass` left before the external call.
        }

        (bool s, bytes memory data) = MSG_VALUE_SIMULATOR.call(abi.encodeWithSignature("deadBeef()"));

        success = s;
        data_first_byte = data[0];
    }
}
