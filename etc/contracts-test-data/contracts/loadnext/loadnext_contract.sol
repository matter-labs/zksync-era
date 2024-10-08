// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;
pragma abicoder v2;

contract LoadnextContract {
    uint internal constant COMPENSATE_READS = 155;
    uint internal constant COMPENSATE_REPEATED_WRITES = 3; // observed: 3.2
    uint internal constant COMPENSATE_INITIAL_WRITES = 0; // observed: 0.1
    uint internal constant COMPENSATE_EVENTS = 6;

    event Event(uint val);
    uint[] readArray;
    uint[] writeArray;

    function saturatingSubtract(uint a, uint b) internal pure returns(uint) {
        return b > a ? 0 : a - b;
    }

    constructor(uint reads) {
        uint _reads = saturatingSubtract(reads, COMPENSATE_READS);
        for (uint i = 0; i < _reads; i++) {
            readArray.push(i);
        }
    }

    function execute(
        uint reads,
        uint initialWrites,
        uint repeatedWrites,
        uint hashes,
        uint events,
        uint maxRecursion,
        uint deploys
    ) external returns (uint) {
        if (maxRecursion > 0) {
            return
                this.execute(
                    reads,
                    initialWrites,
                    repeatedWrites,
                    hashes,
                    events,
                    maxRecursion - 1,
                    deploys
                );
        }

        // apply compensation from observed baseline
        uint _reads = saturatingSubtract(reads, COMPENSATE_READS);
        uint _repeatedWrites = saturatingSubtract(repeatedWrites, COMPENSATE_REPEATED_WRITES);
        uint _initialWrites = saturatingSubtract(initialWrites, COMPENSATE_INITIAL_WRITES);
        uint _events = saturatingSubtract(events, COMPENSATE_EVENTS);

        require(_repeatedWrites <= readArray.length);
        uint sum = 0;

        // Somehow use result of storage read for compiler to not optimize this place.
        for (uint i = 0; i < _repeatedWrites; i++) {
            uint value = readArray[i];
            sum += value;
            readArray[i] = value + 1;
        }
        for (uint i = _repeatedWrites; i < _reads; i++) {
            sum += readArray[i];
        }

        for (uint i = 0; i < _initialWrites; i++) {
            writeArray.push(i);
        }

        for (uint i = 0; i < _events; i++) {
            emit Event(i);
        }

        // Somehow use result of keccak for compiler to not optimize this place.
        for (uint i = 0; i < hashes; i++) {
            sum += uint8(
                keccak256(abi.encodePacked("Message for encoding"))[0]
            );
        }

        for (uint i = 0; i < deploys; i++) {
            Foo foo = new Foo();
        }
        return sum;
    }

    function burnGas(uint256 gasToBurn) external {
        uint256 initialGas = gasleft();
        while (initialGas - gasleft() < gasToBurn) {}
    }
}

contract Foo {
    string public name = "Foo";
}
