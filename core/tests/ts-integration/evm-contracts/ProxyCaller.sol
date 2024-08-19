// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.7.0;

interface ICounterWithParam {
    function increment(uint256 x) external;

    function get() external view returns (uint256);

    function getBytes() external returns (bytes memory);
}

contract ProxyCaller {
    function executeIncrememt(address dest, uint256 x) external{
        ICounterWithParam(dest).increment(x);
    }

    function proxyGet(address dest) external view returns (uint256){
        return ICounterWithParam(dest).get();
    }

    function proxyGetBytes(address dest) external returns(bytes memory returnData){
        return ICounterWithParam(dest).getBytes();
    }
}