// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MemoryExpansion {
    function processLargeArray(uint256[] memory arr) public pure returns (uint256 sum) {
        for(uint i=0; i<arr.length; i++) {
            sum += arr[i];
        }
        return sum;
    }

    function processBytes(bytes memory data) public pure returns (bytes32) {
        return keccak256(data);
    }
}
