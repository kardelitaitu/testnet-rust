// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SimpleFactory {
    event Deployed(address addr, uint256 salt);

    function deploy(uint256 salt, bytes memory bytecode) public returns (address addr) {
        assembly {
            addr := create2(0, add(bytecode, 0x20), mload(bytecode), salt)
        }
        require(addr != address(0), "Create2 failed");
        emit Deployed(addr, salt);
    }
}
