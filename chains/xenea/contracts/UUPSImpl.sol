// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract UUPSImpl {
    uint256 public value;
    uint256 public version = 1;
    address public implementation;

    constructor(uint256 _initialValue) {
        value = _initialValue;
    }

    function getValue() external view returns (uint256) {
        return value;
    }
    
    function setValue(uint256 _value) external {
        value = _value;
    }
    
    function upgradeTo(address newImplementation) external {
        implementation = newImplementation;
    }
    
    function proxiableUUID() external view returns (bytes32) {
        return keccak256("PROXIABLE");
    }
}
