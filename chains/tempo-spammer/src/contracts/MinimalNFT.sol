// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MinimalNFT {
    string public name = "TEMPONFT";
    string public symbol = "TMP";
    uint256 public nextTokenId;
    mapping(uint256 => address) public owners;
    mapping(address => uint256) public balances;
    mapping(address => bool) public isMinter;
    address public admin;

    constructor() {
        admin = msg.sender;
        // Do not auto-grant to force the grantRole step
    }

    function grantRole(address minter) external {
        require(msg.sender == admin, "Not admin");
        isMinter[minter] = true;
    }

    function mint(address to) external {
        require(isMinter[msg.sender], "Not minter");
        uint256 tokenId = nextTokenId++;
        owners[tokenId] = to;
        balances[to]++;
    }
}
