// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

import "./interfaces/ERC20.sol";

contract ZkSyncERC20 is ERC20 {
    constructor(
        string memory name,
        string memory symbol,
        uint8 decimals
    ) ERC20(name, symbol) {
        _setupDecimals(decimals);
    }
}
