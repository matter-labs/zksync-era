// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract StorageTester {
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

    // This test aims to check that the tstore/sstore are writing into separate spaces.
    function testTrasientAndNonTransientStore() external {
        value = 100;

        uint256 x;

        uint256 storedVal;
        uint256 tStoredVal;
        assembly {
            tstore(0, 1)

            storedVal := sload(0)
            tStoredVal := tload(0)
        }

        require(storedVal == 100, "Stored value should be 100");
        require(tStoredVal == 1, "Transient stored value should be 1");
    }


    uint256 constant TSTORE_TEST_KEY = 0xff;

    // We have to use a variable since otherwise the function will be optimized
    // to never do the write in the first place
    function tStoreAndRevert(uint256 value, bool shouldRevert) external {
        uint256 key = TSTORE_TEST_KEY;
        assembly {
            tstore(key, value)
        }

        if(shouldRevert) {
            revert("This method always reverts");
        }
    }

    // We have to use a variable since otherwise the function will be optimized
    // to never do the write in the first place
    function assertTValue(uint256 expectedValue) external {
        uint256 key = TSTORE_TEST_KEY;
        uint256 realValue;
        assembly {
            realValue := tload(key)
        }

        require(realValue == expectedValue, "The value in transient storage is not what was expected");
    }

    function testTstoreRollback() external {
        // Firstly, write the 1000 to the transient storage
        this.tStoreAndRevert(1000, false);

        // Now call and ignore the error
        (bool success, ) = (address(this)).call(abi.encodeWithSignature("tStoreAndRevert(uint256,bool)", uint256(500), true));
        require(!success, "The call should have failed");
        
        this.assertTValue(1000);
    }

    function testTransientStore() external {
        this.testTrasientAndNonTransientStore();
        this.testTstoreRollback();
    }

    fallback() external {}
}
