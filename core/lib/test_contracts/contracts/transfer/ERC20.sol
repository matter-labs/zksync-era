// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

import "@openzeppelin/contracts-v4/token/ERC20/ERC20.sol";

contract TestERC20 is ERC20("Test", "TEST") {
    constructor(uint256 _toMint) {
        _mint(msg.sender, _toMint);
    }
}
