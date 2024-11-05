// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

/// Tests for real EVM emulation (as opposed to mock EVM emulation from `../mock-evm`).
contract EvmEmulationTest {
    modifier validEvmCall() {
        require(address(this).code.length > 0, "contract code length");
        require(gasleft() < (1 << 30), "too much gas");
        _;
    }

    /// Tests simplest successful / reverting call.
    function testCall(bool _shouldRevert) external view validEvmCall {
        require(!_shouldRevert, "requested revert");
    }

    function testCodeHash(bytes32 _expectedHash) external view validEvmCall {
        require(address(this).codehash == _expectedHash, "unexpected code hash");
        require(keccak256(address(this).code) == _expectedHash, "keccak256(code)");
    }

    function testBlockInfo(
        uint _expectedNumber,
        uint _expectedTimestamp,
        bytes32 _expectedPrevHash
    ) external view validEvmCall {
        require(block.chainid == 270, "chain ID");
        require(block.number == _expectedNumber, "block number");
        require(block.timestamp == _expectedTimestamp, "block timestamp");
        require(block.basefee > 0, "block base fee");
        require(block.gaslimit > 0, "block gas limit");
        require(block.coinbase != address(0), "block operator address");
        if (block.number > 1) {
            // The genesis block is processed in a special way
            require(blockhash(block.number - 1) == _expectedPrevHash, "block hash");
        }
    }

    function testMsgInfo(bytes calldata _data) external payable validEvmCall {
        require(msg.sig == EvmEmulationTest.testMsgInfo.selector, "msg.sig");
        require(keccak256(msg.data) == keccak256(abi.encodeCall(EvmEmulationTest.testMsgInfo, _data)), "msg.data");
        require(msg.sender != address(0), "msg.sender");
        require(msg.value == 1 ether, "msg.value");
        require(tx.gasprice == 250000000 wei, "tx.gasprice");
        require(tx.origin == msg.sender, "tx.origin");
    }

    function testRecursion(bool _useFarCalls) external validEvmCall {
        require(recurse(5, _useFarCalls) == 120, "recurse(5)");
        require(recurse(10, _useFarCalls) == 3628800, "recurse(10)");
    }

    function recurse(uint _depth, bool _useFarCalls) public validEvmCall returns (uint) {
        if (_useFarCalls) {
            return (_depth <= 1) ? 1 : this.recurse(_depth - 1, _useFarCalls) * _depth;
        } else {
            return (_depth <= 1) ? 1 : recurse(_depth - 1, _useFarCalls) * _depth;
        }
    }

    ICounter counter;

    function testDeploymentAndCall(bytes32 _expectedCodeHash) external validEvmCall {
        counter = new Counter(1);
        require(address(counter) != address(0), "address(0)");
        require(address(counter) != address(this), "address");
        require(address(counter).codehash == _expectedCodeHash, "code hash");
        require(address(counter).code.length != 0, "code");

        require(counter.get() == 1, "counter.get()");
        counter.increment(2);
        require(counter.get() == 3, "counter.get() after");
    }

    function testCounterCall(uint _expectedInitialValue) external validEvmCall {
        require(address(counter) != address(0), "counter not deployed");

        require(counter.get() == _expectedInitialValue, "counter.get()");
        counter.increment(3);
        require(counter.get() == _expectedInitialValue + 3, "counter.get() after");

        // Test catching reverts from EVM and EraVM contracts.
        try counter.incrementWithRevert(4, true) returns (uint) {
            revert("expected revert");
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("This method always reverts"), "unexpected error");
        }
        require(counter.get() == _expectedInitialValue + 3, "counter.get() after revert");
    }

    function testDeploymentWithPartialRevert(
        bool[] calldata _shouldRevert
    ) external validEvmCall {
        for (uint i = 0; i < _shouldRevert.length; i++) {
            try this.deployThenRevert(
                bytes32(i),
                _shouldRevert[i]
            ) returns(address newAddress) {
                require(!_shouldRevert[i], "unexpected deploy success");
                require(newAddress.code.length > 0, "contract code length");
                require(newAddress.codehash != bytes32(0), "contract code hash");
            } catch Error(string memory reason) {
                require(_shouldRevert[i], "unexpected revert");
                require(keccak256(bytes(reason)) == keccak256("requested revert"), "unexpected error");
            }
        }
    }

    function deployThenRevert(
        bytes32 _salt,
        bool _shouldRevert
    ) external validEvmCall returns (address newAddress) {
        newAddress = address(new Counter{salt: _salt}(0));
        require(newAddress.code.length > 0, "code length");
        require(newAddress.codehash != bytes32(0), "code hash");

        require(!_shouldRevert, "requested revert");
    }
}

/// Counter interface shared with an EraVM counter.
interface ICounter {
    function increment(uint256 _increment) external;
    function incrementWithRevert(uint256 _increment, bool _shouldRevert) external returns (uint256);
    function get() external view returns (uint256);
}

contract Counter is ICounter {
    uint value;

    constructor(uint _initialValue) {
        value = _initialValue;
    }

    function increment(uint256 _increment) external override {
        value += _increment;
    }

    function incrementWithRevert(uint256 _increment, bool _shouldRevert) external override returns (uint256) {
        value += _increment;
        require(!_shouldRevert, "This method always reverts");
        return value;
    }

    function get() external view override returns (uint256) {
        return value;
    }
}
