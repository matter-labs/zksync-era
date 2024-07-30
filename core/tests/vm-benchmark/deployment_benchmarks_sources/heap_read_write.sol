pragma solidity ^0.8.0;

contract HeapBenchmark {
    constructor() {
        uint256 i = 0;
        uint256 n = 1000000;
        uint256[] memory array = new uint256[](n);

        while (true) {
            uint256 previous = 0;

            if (i > 2) {
		uint256 x = array[i-1];
                previous += x + array[x % (i - 1)];
            }
            array[i] = previous;
            i += 1;

            if (i > n) {
                i = 0;
            }
        }
    }
}
