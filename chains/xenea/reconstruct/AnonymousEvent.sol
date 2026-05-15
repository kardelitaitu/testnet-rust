// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AnonymousEvent {
    event AnonymousEvent(uint256 value) anonymous;
    event NamedEvent(uint256 value);

    function emitAnonymous(uint256 value) public {
        emit AnonymousEvent(value);
    }

    function emitNamed(uint256 value) public {
        emit NamedEvent(value);
    }
}
