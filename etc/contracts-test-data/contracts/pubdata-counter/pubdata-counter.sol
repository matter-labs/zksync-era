// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract PubdataCounter {
    uint256 public value;

    // Will cause 65-byte pubdata to be published
    uint256 constant BIG_VALUE = 0x0fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff;

    // Will cause 34-byte pubdata to be published
    uint256 constant SMALL_VALUE = 1;

    function simpleWrite() external {
        value = BIG_VALUE;
    }

    function resettingWrite() external {
        value = BIG_VALUE;
        value = SMALL_VALUE;
    }

    // We have to use a variable since otherwise the function will be optimized
    // to never do the write in the first place
    function writeAndRevert(bool shouldRevert) external {
        value = BIG_VALUE;

        if(shouldRevert) {
            revert("This method always reverts");
        }
    }
    
    function resettingWriteViaRevert() external {
        value = SMALL_VALUE;
        (bool success ,) = (address(this)).call(abi.encodeWithSignature("writeAndRevert(bool)", (true)));
        value = SMALL_VALUE;
    }

    fallback() external {}
}
