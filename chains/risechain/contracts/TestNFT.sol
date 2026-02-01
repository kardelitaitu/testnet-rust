// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract TestNFT {
    string public name;
    string public symbol;
    uint256 public totalSupply;
    mapping(uint256 => address) public ownerOf;
    mapping(uint256 => string) public tokenURI;

    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);

    constructor(string memory _name, string memory _symbol) {
        name = _name;
        symbol = _symbol;
    }

    function supportsInterface(bytes4 interfaceId) public pure returns (bool) {
        return interfaceId == 0x80ac58cd || // ERC721
               interfaceId == 0x5b5e139f || // ERC721Metadata
               interfaceId == 0x01ffc9a7;   // ERC165
    }

    function mint(address to, string memory uri) public returns (uint256) {
        uint256 tokenId = totalSupply + 1;
        totalSupply++;
        ownerOf[tokenId] = to;
        tokenURI[tokenId] = uri;
        emit Transfer(address(0), to, tokenId);
        return tokenId;
    }

    function transferFrom(address from, address to, uint256 tokenId) public {
        require(ownerOf[tokenId] == from, "Not owner");
        ownerOf[tokenId] = to;
        emit Transfer(from, to, tokenId);
    }

    function safeTransferFrom(address from, address to, uint256 tokenId) public {
        transferFrom(from, to, tokenId);
    }

    function safeTransferFrom(address from, address to, uint256 tokenId, bytes memory) public {
        transferFrom(from, to, tokenId);
    }
}
