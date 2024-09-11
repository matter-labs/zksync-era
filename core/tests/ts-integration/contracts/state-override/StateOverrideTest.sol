// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

pragma solidity ^0.8.0;

contract StateOverrideTest {
    uint256 public someValue;
    uint256 public anotherValue;
    uint256 public initialValue = 100;

    function setValue(uint256 value) public {
        someValue = value;
    }

    function setAnotherValue(uint256 value) public {
        anotherValue = value;
    }

    function increment(uint256 value) public view returns (uint256) {
        require(someValue > 0, "Initial state not set");
        return someValue + value;
    }

    function sumValues() public view returns (uint256) {
        require(someValue > 0 && anotherValue > 0, "Initial state not set");
        return someValue + anotherValue + initialValue;
    }
}
