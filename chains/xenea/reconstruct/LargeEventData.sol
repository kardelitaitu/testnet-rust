// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract LargeEventData {
    event LargeData(bytes data);

    function emitLargeData(bytes memory data) public {
        emit LargeData(data);
    }
}
