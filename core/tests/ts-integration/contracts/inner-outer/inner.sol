pragma solidity ^0.8.0;

contract Inner {
    uint256 public value;

    constructor(uint256 _value) {
        value = _value;
    }
}
