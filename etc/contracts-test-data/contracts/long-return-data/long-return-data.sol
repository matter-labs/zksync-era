// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

contract LongReturnData{
    function longReturnData() external returns (bool, bytes memory) {
        // do some recursion, let's have more layers
        (bool success, bytes memory _tmp) = this.longReturnData{gas: 79500000}();
        require(success == false); // they should fail by design
        assembly {
            return(0, 0xffffffffffffffff)
        }
    }
}
