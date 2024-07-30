pragma solidity ^0.8.0;

contract HeapBenchmark {
    constructor() {
        uint256 v1 = 0;
        uint256 v2 = 0;
        uint256 n = 1000000;
        uint256[] memory array = new uint256[](1);

        assembly {
            for { let j := 0 } lt(j, n) { j := add(j, 1) } {
                v1 := mload(add(array, mod(mul(j, j), n)))
                v2 := mload(add(array, sub(j, 1)))
                mstore(add(array, j), add(add(v1, v2), 42))
            }
        }
    }
}
