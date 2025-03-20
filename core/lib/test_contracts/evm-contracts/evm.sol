// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.0;

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

interface IGasTester {
    error TooMuchGas(uint expected, uint actual);
    error TooFewGas(uint expected, uint actual);

    function testGas(uint _expectedGas, bool _consumeAllGas) external view;
}

/// Tests for real EVM emulation (as opposed to mock EVM emulation from `../mock-evm`).
contract EvmEmulationTest is IGasTester {
    event SimpleEvent();

    event ComplexEvent(
        bytes32 indexed someHash,
        string additionalData
    );

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

    /// Only expensive opcodes are checked to eliminate noise.
    function testGasManagement(uint _gasLimit) external validEvmCall returns(bytes32 dummyHash) {
        uint initialGas = gasleft();
        require(initialGas > 0, "initial gas == 0");
        require(initialGas <= _gasLimit, "initial gas");

        // Balance, self-balance
        initialGas = gasleft();
        uint value = address(0x11111111).balance;
        uint gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 2600, "cold balance");
        dummyHash = keccak256(abi.encode(dummyHash, value));

        initialGas = gasleft();
        value = address(0x11111111).balance;
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 100, "warm balance");
        dummyHash = keccak256(abi.encode(dummyHash, value));

        initialGas = gasleft();
        value = address(this).balance;
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 5, "self balance");
        dummyHash = keccak256(abi.encode(dummyHash, value));

        // code hash, code size
        initialGas = gasleft();
        value = address(0x11111112).code.length;
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 2600, "cold code size");
        dummyHash = keccak256(abi.encode(dummyHash, value));

        initialGas = gasleft();
        value = address(0x11111112).code.length;
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 100, "warm code size");
        dummyHash = keccak256(abi.encode(dummyHash, value));

        initialGas = gasleft();
        bytes32 hash = address(0x11111113).codehash;
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 2600, "cold code hash");
        dummyHash = keccak256(abi.encode(dummyHash, hash));

        initialGas = gasleft();
        hash = address(0x11111113).codehash;
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 100, "warm code hash");
        dummyHash = keccak256(abi.encode(dummyHash, hash));

        // Storage load
        initialGas = gasleft();
        assembly {
            value := sload(0x123456)
        }
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 2100, "cold sload");
        dummyHash = keccak256(abi.encode(dummyHash, value));

        initialGas = gasleft();
        assembly {
            value := sload(0x123456)
        }
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 100, "warm sload");
        dummyHash = keccak256(abi.encode(dummyHash, value));

        // Storage store
        initialGas = gasleft();
        assembly {
            sstore(0x654321, 1)
        }
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 2100 /* cold */ + 20000 /* 0 -> non-zero */, "cold sstore");

        initialGas = gasleft();
        assembly {
            sstore(0x654321, 2)
        }
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 100 /* dirty slot */, "non-zero warm sstore");

        initialGas = gasleft();
        assembly {
            sstore(0x1234, 0)
        }
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 2100 /* cold */ + 100 /* 0 -> 0 */, "cold no-op sstore");

        // Transient load / store
        initialGas = gasleft();
        assembly {
            value := tload(0x1234)
        }
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 100, "tload");
        dummyHash = keccak256(abi.encode(dummyHash, value));

        initialGas = gasleft();
        assembly {
            tstore(0x1234, 1)
        }
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 100, "tstore");

        // Events
        initialGas = gasleft();
        emit SimpleEvent();
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 375 + 375 * 1 /* topic count */ + 10 /* additional PUSH* cost? */, "event");

        // keccak256
        bytes memory data = bytes("test string.............................................................");
        initialGas = gasleft();
        hash = keccak256(data);
        gasCost = initialGas - gasleft();
        _requireGasCost(gasCost, 30 + 6 * (data.length + 31) / 32 + 10 /* additional PUSH* cost? */, "keccak256");
        dummyHash = keccak256(abi.encode(dummyHash, value));
    }

    uint private constant GAS_TOLERANCE = 20;

    function _requireGasCost(uint _actualCost, uint _expectedCost, string memory op) internal pure {
        require(_actualCost >= _expectedCost, string.concat(op, " cost too low"));
        require(_actualCost <= _expectedCost + GAS_TOLERANCE, string.concat(op, " cost too high"));
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

    function testDeployment(address _expectedAddress) external validEvmCall {
        Counter newCounter = new Counter(1);
        require(address(newCounter) == _expectedAddress, "address");
    }

    function testCreate2Deployment(
        bytes32 _salt,
        uint256 _constructorArg,
        address _expectedAddress
    ) external validEvmCall {
        Counter newCounter = new Counter{salt: _salt}(_constructorArg);
        require(address(newCounter) == _expectedAddress, "address");
    }

    // Gas passed to failed CREATE calls. Since all available gas is burned in the failed call, we don't want
    // to use the default 63/64 of all gas.
    uint private constant CREATE_GAS = 1000000;

    function testReusingCreateAddress(address _expectedAddress) external validEvmCall {
        require(gasleft() > 2 * CREATE_GAS, "too few gas");

        try this.deployThenRevert{gas: CREATE_GAS}(true) {
            revert("expected revert");
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("requested revert"), "unexpected error");
        }
        address deployedAddress = this.deployThenRevert(false);
        require(deployedAddress == _expectedAddress, "deployedAddress");
    }

    function deployThenRevert(bool _shouldRevert) external validEvmCall returns (address newAddress) {
        newAddress = address(new Counter(0));
        require(!_shouldRevert, "requested revert");
    }

    function testReusingCreate2Salt() external validEvmCall {
        require(gasleft() > 3 * CREATE_GAS, "too few gas");

        this.deploy2ThenRevert(bytes32(0), false);
        try this.deploy2ThenRevert{gas: CREATE_GAS}(bytes32(0), false) {
            revert("expected revert");
        } catch {
            // failed as expected
        }

        try this.deploy2ThenRevert{gas: CREATE_GAS}(hex"01", true) {
            revert("expected revert");
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("requested revert"), "unexpected error");
        }
        // This should succeed because the previous deployment was reverted
        this.deploy2ThenRevert(hex"01", false);
    }

    function deploy2ThenRevert(
        bytes32 _salt,
        bool _shouldRevert
    ) external validEvmCall returns (address newAddress) {
        newAddress = address(new Counter{salt: _salt}(0));
        require(newAddress.code.length > 0, "code length");
        require(newAddress.codehash != bytes32(0), "code hash");

        require(!_shouldRevert, "requested revert");
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
        try counter.incrementWithRevert(4, true) {
            revert("expected revert");
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("This method always reverts"), "unexpected error");
        }
        require(counter.get() == _expectedInitialValue + 3, "counter.get() after revert");
    }

    uint private constant GAS_TO_SEND = 100000;

    function testFarCalls(IGasTester _tester, bool _isEvmTester) external view validEvmCall {
        uint gasMultiplier = _isEvmTester ? 1 : 5;
        uint currentGas = gasleft();
        if (_isEvmTester) {
            _tester.testGas((currentGas * 63 / 64) * gasMultiplier, false);
        }

        currentGas = gasleft();
        require(currentGas > 2 * GAS_TO_SEND, "too few gas left");
        _tester.testGas{gas: GAS_TO_SEND}(GAS_TO_SEND * gasMultiplier, false);

        // Attempt to send "infinite" gas from the stipend (shouldn't work)
        currentGas = gasleft();
        if (_isEvmTester) {
            _tester.testGas{gas: 1 << 30}((currentGas * 63 / 64) * gasMultiplier, false);
        }

        currentGas = gasleft();
        require(currentGas > 2 * GAS_TO_SEND, "too few gas left");
        try _tester.testGas{gas: GAS_TO_SEND}(GAS_TO_SEND * gasMultiplier, true) {
            revert("expected out-of-gas");
        } catch {
            require(currentGas - gasleft() > GAS_TO_SEND, "gas consumed");
        }
    }

    function testGas(uint _expectedGas, bool _consumeAllGas) external view override validEvmCall {
        uint currentGas = gasleft();
        if (currentGas > _expectedGas) revert TooMuchGas(_expectedGas, currentGas);
        if (currentGas < _expectedGas - 2000) revert TooFewGas(_expectedGas, currentGas);

        if (_consumeAllGas) {
            bytes32 hash;
            while (uint(hash) != 1) {
                hash = keccak256(bytes.concat(hash));
            }
        }
    }

    function testDeploymentWithPartialRevert(
        bool[] calldata _shouldRevert
    ) external validEvmCall {
        for (uint i = 0; i < _shouldRevert.length; i++) {
            try this.deploy2ThenRevert(
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

    function testEvents() external validEvmCall {
        emit SimpleEvent();
        string memory data = "Test";
        emit ComplexEvent(keccak256(bytes(data)), data);
    }

    function testSha256(bytes calldata _input, bytes32 _expected) external validEvmCall {
        require(sha256(_input) == _expected, "sha256");
    }

    function testEcrecover(
        bytes32 _messageDigest,
        uint8 _v,
        bytes32 _r,
        bytes32 _s,
        address _expectedAddress
    ) external validEvmCall {
        address actualAddress = ecrecover(_messageDigest, _v, _r, _s);
        if (_expectedAddress != address(0)) {
            // Check success to simplify debugging
            require(actualAddress != address(0), "ecrecover failed");
        }
        require(actualAddress == _expectedAddress, "ecrecover");
    }
}
