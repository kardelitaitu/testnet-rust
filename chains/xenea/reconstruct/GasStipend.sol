// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract GasStipend {
    function callWithGas(uint256 gasAmount) public view returns (bool success, bytes memory data) {
        // Enforce the gas stipend check
        if (gasleft() < gasAmount) {
            return (false, "Not enough gas");
        }
        return (true, "");
    }
}
