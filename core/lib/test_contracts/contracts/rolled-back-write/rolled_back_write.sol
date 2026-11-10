// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8.0;

interface ISystemContext {
    function getCurrentPubdataSpent() external view returns (uint256);
}

/// `inner` writes `s` and reverts, so the write leaves no storage log, yet its price (from `s`'s
/// pre-state) is read through the pubdata counter and persisted in `t`.
contract RolledBackWrite {
    // Pre-set to a non-zero value by the test.
    uint256 public s;
    uint256 public t;

    function outer(uint256 x) external {
        try this.inner(x) {} catch (bytes memory r) {
            t = abi.decode(r, (uint256));
        }
    }

    function inner(uint256 x) external {
        // Plain SSTORE: no SLOAD of `s`, and `x` differs from its current value.
        s = x;
        bytes memory out = abi.encode(ISystemContext(address(0x800b)).getCurrentPubdataSpent());
        assembly {
            revert(add(out, 32), mload(out))
        }
    }
}
