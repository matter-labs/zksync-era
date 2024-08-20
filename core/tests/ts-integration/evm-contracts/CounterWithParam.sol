// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.7.0;

contract CounterWithParam {
    uint256 value;


    constructor(uint256 _startingValue) {
        value = _startingValue;
    }

    function increment(uint256 x) public {
        value += x;
    }
    
    function incrementWithRevertPayable(uint256 x, bool shouldRevert) payable public returns (uint256) {
        return incrementWithRevert(x, shouldRevert);
    }

    function incrementWithRevert(uint256 x, bool shouldRevert) public returns (uint256) {
        value += x;
        if(shouldRevert) {
            revert("This method always reverts");
        }
        return value;
    }

    function set(uint256 x) public {
        value = x;
    }

    function get() public view returns (uint256) {
        return value;
    }

    function getBytes() public returns (bytes memory) {
        return "Testing";
    }
}
