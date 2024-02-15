// SPDX-License-Identifier: MIT

pragma solidity >=0.8.1;
pragma abicoder v2;

// import Foo.sol from current directory
import "./Foo.sol";

contract Import {
    // Initialize Foo.sol
    Foo public foo = new Foo();

    // Initialize Foo.sol
    Foo public foo2 = new Foo{salt: bytes32(bytes1(0x00))}();
}