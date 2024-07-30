pragma solidity ^0.8.0;

contract HeapBenchmark {
    constructor() {
        uint256 n = 1000000;
        uint256[] memory array = new uint256[](n);
        
        for (uint i = 0; i < n; i++) {
            uint256 previous = 0;

            if (i > 2) {
                previous += array[i-1] + array[i-2];
            }
            array[i] = previous + i + 999;
        }
    }
}
