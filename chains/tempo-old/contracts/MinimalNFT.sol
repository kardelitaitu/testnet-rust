
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MinimalNFT {
    string public name;
    string public symbol;
    uint256 public nextTokenId;
    mapping(uint256 => address) public owners;
    mapping(address => uint256) public balances;

    constructor(string memory _name, string memory _symbol) {
        name = _name;
        symbol = _symbol;
    }

    function mint(address to) external {
        uint256 tokenId = nextTokenId++;
        owners[tokenId] = to;
        balances[to]++;
    }
}
