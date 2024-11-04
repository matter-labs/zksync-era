// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

/// Tests for real EVM emulation (as opposed to mock EVM emulation from `../mock-evm`).
contract EvmEmulationTest {
    modifier validEvmCall() {
        require(address(this).code.length > 0, "contract code length");
        require(gasleft() < (1 << 30), "too much gas");
        _;
    }

    /// Tests simplest successful / reverting call.
    function testCall(bool _shouldRevert) external view validEvmCall {
        require(!_shouldRevert, "requested revert");
    }

    function testRecursion(bool _useFarCalls) external validEvmCall {
        require(recurse(5, _useFarCalls) == 120, "recurse(5)");
        require(recurse(10, _useFarCalls) == 3628800, "recurse(10)");
    }

    function recurse(uint _depth, bool _useFarCalls) public validEvmCall returns (uint) {
        if (_useFarCalls) {
            return (_depth <= 1) ? 1 : this.recurse(_depth - 1, _useFarCalls) * _depth;
        } else {
            return (_depth <= 1) ? 1 : recurse(_depth - 1, _useFarCalls) * _depth;
        }
    }
}
