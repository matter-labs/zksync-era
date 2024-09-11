pragma solidity ^0.8.0;

import "./inner.sol";

contract Outer {
    Inner public innerContract;

    constructor(uint256 _value) {
        innerContract = new Inner(_value);
    }
}
