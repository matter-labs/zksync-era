// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IERC20 {
    function transfer(address to, uint256 value) external returns (bool);
    function transferFrom(address from, address to, uint256 value) external returns (bool);
}

contract Swap {

    address public token1Address;
    address public token2Address;

    constructor(address _token1Address, address _token2Address) {
        token1Address = _token1Address;
        token2Address = _token2Address;
    }

    function swap(uint256 amount) public payable {
        IERC20(token1Address).transferFrom(msg.sender, address(this), amount);
        IERC20(token2Address).transfer(msg.sender, amount);
    }
}