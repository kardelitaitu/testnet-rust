// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract CalldataSize {
    bytes32 public dataHash;

    function storeData(bytes memory data) public {
        dataHash = keccak256(data);
    }

    function getDataHash() public view returns (bytes32) {
        return dataHash;
    }
}
