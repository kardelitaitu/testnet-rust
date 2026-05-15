// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract IndexedTopics {
    event MultiTransfer(address indexed from, address indexed to, uint256 indexed id1, uint256 id2);

    function emitMultiIndexed(address from, address to, uint256 id1, uint256 id2) public {
        emit MultiTransfer(from, to, id1, id2);
    }
}
