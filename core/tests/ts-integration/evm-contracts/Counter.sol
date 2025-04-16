// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

contract Counter {
    uint256 value;

    constructor(bool _revert) {
        require(!_revert, "requested revert");
    }

    function increment(uint256 _x) public {
        value += _x;
    }

    function get() public view returns (uint256) {
        return value;
    }
}

contract CounterFactory {
    mapping(bytes32 => Counter) private deployedCounters;

    function getAddress(bytes32 _salt) external view returns (address) {
        return address(deployedCounters[_salt]);
    }

    function deploy(bytes32 _salt) external {
        deployedCounters[_salt] = new Counter{salt: _salt}(false);
    }
}
