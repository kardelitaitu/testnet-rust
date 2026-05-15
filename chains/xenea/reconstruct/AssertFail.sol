// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AssertFail {
    uint256 public value;

    function assertCheck(uint256 v) public {
        value = v;
        assert(v != 0);
    }

    function requireCheck(uint256 v) public {
        value = v;
        require(v != 0);
    }

    function getValue() public view returns (uint256) {
        return value;
    }
}
