// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Impl {
    address public admin;
    address public implementation;
    uint256 public value;

    constructor(address _admin) {
        admin = _admin;
    }
    
    function getAdmin() external view returns (address) {
        return admin;
    }
    
    function getImplementation() external view returns (address) {
        return implementation;
    }
    
    function setValue(uint256 _value) external {
        value = _value;
    }
    
    function getValue() external view returns (uint256) {
        return value;
    }
}
