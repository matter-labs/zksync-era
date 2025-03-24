// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

contract Counter {
    uint256 value;

    function increment(uint256 x) external {
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

    function get() public view returns (uint256) {
        return value;
    }
}

contract CounterFactory {
    function testReusingCreateAddress(address _expectedAddress) external {
        try this.deployThenRevert(true) {
            revert("expected revert");
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("requested revert"), "unexpected error");
        }
        address deployedAddress = this.deployThenRevert(false);
        require(deployedAddress == _expectedAddress, "deployedAddress");
    }

    function deployThenRevert(bool _shouldRevert) external returns (address newAddress) {
        newAddress = address(new Counter());
        require(!_shouldRevert, "requested revert");
    }

    function testReusingCreate2Salt() external {
        this.deploy2ThenRevert(bytes32(0), false);
        try this.deploy2ThenRevert(bytes32(0), false) {
            revert("expected create2 to fail");
        } catch {
            // failed as expected
        }

        try this.deploy2ThenRevert(hex"01", true) {
            revert("expected revert");
        } catch {
            // failed as expected
        }
        // This should succeed because the previous deployment was reverted
        this.deploy2ThenRevert(hex"01", false);
    }

    function deploy2ThenRevert(bytes32 _salt, bool _shouldRevert) external {
        new Counter{salt: _salt}();
        require(!_shouldRevert, "requested revert");
    }
}
