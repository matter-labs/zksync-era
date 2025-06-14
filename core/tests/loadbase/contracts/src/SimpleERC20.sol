// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

contract SimpleERC20 {
    string  public name;
    string  public symbol;
    uint8   public constant decimals = 18;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;

    constructor(string memory _name, string memory _symbol) {
        name   = _name;
        symbol = _symbol;
    }

    function mint(address to, uint256 amount) public {
        totalSupply   += amount;
        balanceOf[to] += amount;
    }

    function transfer(address to, uint256 amount) public returns (bool) {
        uint256 bal = balanceOf[msg.sender];
        require(bal >= amount, "balance");
        unchecked {
            balanceOf[msg.sender] = bal - amount;
            balanceOf[to]        += amount;
        }
        return true;
    }
}
