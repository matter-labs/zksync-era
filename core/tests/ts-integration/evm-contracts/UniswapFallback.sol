// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

interface IUniswapV2ERC20 {
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event Transfer(address indexed from, address indexed to, uint256 value);

    function name() external pure returns (string memory);

    function symbol() external pure returns (string memory);

    function decimals() external pure returns (uint8);

    function totalSupply() external returns (uint256);

    function balanceOf(address owner) external returns (uint256);

    function allowance(address owner, address spender) external returns (uint256);

    function approve(address spender, uint256 value) external returns (bool);

    function transfer(address to, uint256 value) external returns (bool);

    function transferFrom(
        address from,
        address to,
        uint256 value
    ) external returns (bool);

    function DOMAIN_SEPARATOR() external returns (bytes32);

    function PERMIT_TYPEHASH() external pure returns (bytes32);

    function nonces(address owner) external returns (uint256);

    function permit(
        address owner,
        address spender,
        uint256 value,
        uint256 deadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external;
}

interface IUniswapV2Pair {
    event Mint(address indexed sender, uint256 amount0, uint256 amount1);
    event Burn(address indexed sender, uint256 amount0, uint256 amount1, address indexed to);
    event Swap(
        address indexed sender,
        uint256 amount0In,
        uint256 amount1In,
        uint256 amount0Out,
        uint256 amount1Out,
        address indexed to
    );
    event Sync(uint112 reserve0, uint112 reserve1);

    function MINIMUM_LIQUIDITY() external pure returns (uint256);

    function factory() external returns (address);

    function token0() external returns (address);

    function token1() external returns (address);

    function getReserves()
        external
        returns (
            uint112 reserve0,
            uint112 reserve1,
            uint32 blockTimestampLast
        );

    function price0CumulativeLast() external returns (uint256);

    function price1CumulativeLast() external returns (uint256);

    function kLast() external returns (uint256);

    function mint(address to) external returns (uint256 liquidity);

    function burn(address to) external returns (uint256 amount0, uint256 amount1);

    function swap(
        uint256 amount0Out,
        uint256 amount1Out,
        address to,
        bytes calldata data
    ) external;

    function skim(address to) external;

    function sync() external;

    function initialize(address, address) external;
}

contract UniswapFallback {
    IUniswapV2Pair public uniswapPair;
    IUniswapV2ERC20 public uniswapPair2;
    address public alice_address;

    function setUniswapAddress(address _uniswap_address) public {
        uniswapPair = IUniswapV2Pair(_uniswap_address);
        uniswapPair2 = IUniswapV2ERC20(_uniswap_address);
    }

    function setAliceAddress(address _alice_address) public {
        alice_address = _alice_address;
    }

    // Fallback function
    fallback() external {
        // Implement any logic you want the contract to perform when it receives Ether
        // This function will be called when the contract receives Ether and no other function matches the call data
        uniswapPair.mint(alice_address);
        uniswapPair.swap(0, 5000, alice_address, "0x");
        uint256 balance = uniswapPair2.balanceOf(alice_address);
        //uniswapPair2.transfer(address(uniswapPair),balance);
        //uniswapPair.burn(alice_address);
    }
}
