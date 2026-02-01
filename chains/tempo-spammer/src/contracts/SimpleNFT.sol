// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SimpleNFT {
    string public name = "NFT";
    string public symbol = "NFT";
    uint256 public nextTokenId;
    mapping(uint256 => address) public owners;

    function mint(address to) external {
        uint256 tokenId = nextTokenId++;
        owners[tokenId] = to;
    }
}
