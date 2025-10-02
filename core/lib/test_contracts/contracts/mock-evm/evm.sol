// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

interface IGasTester {
    error TooMuchGas(uint expected, uint actual);
    error TooFewGas(uint expected, uint actual);

    function testGas(uint _expectedGas, bool _consumeAllGas) external view;
}

/// EraVM part of some EVM emulation tests.
contract EraVmTester is IGasTester {
    uint private constant GAS_TO_SEND = 500000;

    function testFarCalls(IGasTester _tester, bool _isEvmTester) external view {
        uint gasDivisor = _isEvmTester ? 5 : 1;
        uint currentGas = gasleft();
        _tester.testGas((currentGas * 63 / 64) / gasDivisor, false);

        currentGas = gasleft();
        require(currentGas > GAS_TO_SEND * 2, "too few gas left");
        _tester.testGas{gas: GAS_TO_SEND}(GAS_TO_SEND / gasDivisor, false);

        // Attempt to send "infinite" gas from the stipend (shouldn't work)
        currentGas = gasleft();
        _tester.testGas{gas: 1 << 30}((currentGas * 63 / 64) / gasDivisor, false);

        currentGas = gasleft();
        require(currentGas > 2 * GAS_TO_SEND, "too few gas left");
        try _tester.testGas{gas: GAS_TO_SEND}(GAS_TO_SEND / gasDivisor, true) {
            revert("expected out-of-gas");
        } catch {
            require(currentGas - gasleft() > GAS_TO_SEND, "gas consumed");
        }
    }

    function testGas(uint _expectedGas, bool _consumeAllGas) external view override {
        uint currentGas = gasleft();
        if (currentGas > _expectedGas) revert TooMuchGas(_expectedGas, currentGas);
        if (currentGas < _expectedGas - 1500) revert TooFewGas(_expectedGas, currentGas);

        if (_consumeAllGas) {
            bytes32 hash;
            while (uint(hash) != 1) {
                hash = keccak256(bytes.concat(hash));
            }
        }
    }
}
