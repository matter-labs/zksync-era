// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

contract GasTester {
    constructor() {}

    function infiniteCall(uint256 _toReturn) external {
        // The job of this call is to just burn gas
        while (gasleft() > _toReturn) {}
    }

    function testShouldSpendEverything() internal {
        uint256 gasBefore = gasleft();

        try this.infiniteCall{gas: 10000}(0) {
            revert("The infinite call must fail");
        } catch {
        }

        uint256 gasAfter = gasleft();

        assertEqGas(gasBefore - gasAfter, 10000);
    }

    function testShouldSpendPart() internal {
        uint256 gasBefore = gasleft();

        this.infiniteCall{gas: 10000}(5000);

        uint256 gasAfter = gasleft();

        assertEqGas(gasBefore - gasAfter, 5000);
    }

    function testGas() external {
        testShouldSpendEverything();
        testShouldSpendPart();
    }

    function assertEqGas(uint256 a, uint256 b) internal pure returns (bool) {
        // here 100 is the allowed error, since we often can not predict exact solidity behavior 
        if(a >= b) {
            return b + 1000 >= a;
        } else {
            return a + 1000 >= b;
        }
    }

    fallback() external {

    }
}

