// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract RevertReason {
    uint256 public state;
    error CustomRevert();

    function revertWithMessage(string memory message) public pure {
        require(bytes(message).length == 0, message);
    }

    function revertWithCustomError() public pure {
        revert CustomRevert();
    }

    function getState() public view returns (uint256) {
        return state;
    }
}
