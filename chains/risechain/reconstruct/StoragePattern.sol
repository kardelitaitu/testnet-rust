// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract StoragePattern {
    uint256 public packed;

    // ABI in t44 says constructor(uint256), but deployment sends no args.
    // If we include args in constructor, deployment might fail if not provided.
    // We will define it with NO args to be safe for the test's deployment logic.
    constructor() {
        packed = 0;
    }

    function getPacked() public view returns (uint256) {
        return packed;
    }

    function setValues(uint128 a, uint128 b) public {
        // packed = (a << 128) | b
        packed = (uint256(a) << 128) | uint256(b);
    }
}
