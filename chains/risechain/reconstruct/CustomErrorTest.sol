// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract CustomErrorTest {
    uint256 public data;
    error MyCustomError();

    function testError(bool shouldFail) public {
        if (shouldFail) {
            revert MyCustomError();
        }
        data = 45;
    }

    function getData() public view returns (uint256) {
        return data; // verify initial value (0)
    }
}
