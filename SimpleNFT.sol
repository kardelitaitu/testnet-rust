// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SimpleNFT {
    string public name;
    string public symbol;
    address public owner;
    uint256 public tokenCount;
    mapping(uint256 => address) public tokenOwner;
    mapping(address => uint256) public balanceOf;

    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);

    constructor(string memory _name, string memory _symbol) {
        name = _name;
        symbol = _symbol;
        owner = msg.sender;
    }

    function mint(address to) external returns (uint256) {
        require(msg.sender == owner, "Not owner");
        tokenCount++;
        uint256 tokenId = tokenCount;
        tokenOwner[tokenId] = to;
        balanceOf[to]++;
        emit Transfer(address(0), to, tokenId);
        return tokenId;
    }

    function transferFrom(address from, address to, uint256 tokenId) external {
        require(tokenOwner[tokenId] == from, "Not owner");
        require(msg.sender == from || msg.sender == owner, "Not authorized");
        tokenOwner[tokenId] = to;
        balanceOf[from]--;
        balanceOf[to]++;
        emit Transfer(from, to, tokenId);
    }
}
