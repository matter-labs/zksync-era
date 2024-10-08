// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;
pragma abicoder v2;

contract LoadnextContract {
    event Event(uint val);
    uint[] readArray;
    uint[] writeArray;

    constructor (uint reads) {
        for (uint i = 0; i < reads; i++) {
            readArray.push(i);
        }
    }

    function execute(uint reads, uint writes, uint hashes, uint events, uint max_recursion, uint deploys) external returns(uint) {
        if (max_recursion > 0) {
            return this.execute(reads, writes, hashes, events, max_recursion - 1, deploys);
        }

        uint sum = 0;

        // Somehow use result of storage read for compiler to not optimize this place.
        for (uint i = 0; i < reads; i++) {
            sum += readArray[i];
        }

        for (uint i = 0; i < writes; i++) {
            writeArray.push(i);
        }

        for (uint i = 0; i < events; i++) {
            emit Event(i);
        }

        // Somehow use result of keccak for compiler to not optimize this place.
        for (uint i = 0; i < hashes; i++) {
            sum += uint8(keccak256(abi.encodePacked("Message for encoding"))[0]);
        }

        for (uint i = 0; i < deploys; i++) {
            Foo foo = new Foo();
        }
        return sum;
    }

    function burnGas(uint256 gasToBurn) external {
        uint256 initialGas = gasleft();
        while(initialGas - gasleft() < gasToBurn) {}
    }
}

contract Foo {
    string public name = "Foo";
}
