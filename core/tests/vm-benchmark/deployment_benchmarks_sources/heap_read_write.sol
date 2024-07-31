pragma solidity ^0.8.0;

contract HeapBenchmark {
    constructor() {
        uint256 v1 = 0;
        uint256 v2 = 0;
        uint256 n = 16000;
        uint256[] memory array = new uint256[](1);

        assembly {
            mstore(add(array, sub(n, 1)), 4242)

            let j := 0
            for {} lt(j, n) {} {
                v1 := mload(add(array, mod(mul(j, j), n)))
                v2 := mload(add(array, sub(j, 1)))
                mstore(add(array, j), add(add(v1, v2), 42))
                j := add(j, 1) 
                if gt(j, sub(n, 1)) {
                    j := 0
                }
            }
        }
    }
}
