pragma solidity ^0.8.0;

// SPDX-License-Identifier: MIT OR Apache-2.0

contract SimpleRequire {
    error TestError(uint256 one, uint256 two, uint256 three, string data);

    function new_error() public pure {
        revert TestError({one: 1, two: 2, three: 1, data: "data"});
    }

    function require_short() public pure {
        require(false, "short");
    }

    function require_long() public pure {
        require(
            false,
            'longlonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglong'
        );
    }
}
