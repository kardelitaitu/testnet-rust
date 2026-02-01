// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/**
 * @title ViralNFT
 * @dev A minimal, viral ERC721 implementation for caching and minting experiments.
 * Zero dependencies, highly optimized for deployment speed.
 */
contract ViralNFT {
    string public name;
    string public symbol;
    
    mapping(uint256 => address) public _owners;
    mapping(address => uint256) public _balances;
    mapping(uint256 => address) public _tokenApprovals;
    mapping(address => mapping(address => bool)) public _operatorApprovals;

    uint256 public totalSupply;
    uint256 public nextClaimId;
    uint256 public constant MAX_SUPPLY = 100;

    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
    event Approval(address indexed owner, address indexed approved, uint256 indexed tokenId);
    event ApprovalForAll(address indexed owner, address indexed operator, bool approved);

    constructor(string memory _name, string memory _symbol) {
        name = _name;
        symbol = _symbol;
        // Pre-mint to contract so it "holds" the inventory
        for (uint256 i = 0; i < MAX_SUPPLY; i++) {
            _mint(address(this), i);
        }
        totalSupply = MAX_SUPPLY;
    }

    function balanceOf(address owner) public view returns (uint256) {
        require(owner != address(0), "ERC721: balance query for the zero address");
        return _balances[owner];
    }

    function ownerOf(uint256 tokenId) public view returns (address) {
        address owner = _owners[tokenId];
        require(owner != address(0), "ERC721: owner query for nonexistent token");
        return owner;
    }

    function tokenURI(uint256 tokenId) public view returns (string memory) {
        require(_owners[tokenId] != address(0), "Nonexistent token");
        // Returns base64 metadata: {"name": "ViralNFT", "description": "Viral", "image": "https://tempo.xyz/logo.png"}
        return "data:application/json;base64,eyJuYW1lIjogIlZpcmFsTkZUIiwgImRlc2NyaXB0aW9uIjogIlZpcmFsIiwgImltYWdlIjogImh0dHBzOi8vdGVtcG8ueHl6L2xvZ28ucG5nIn0=";
    }

    function approve(address to, uint256 tokenId) public {
        address owner = ownerOf(tokenId);
        require(to != owner, "ERC721: approval to current owner");
        require(msg.sender == owner || isApprovedForAll(owner, msg.sender), "ERC721: approve caller is not owner nor approved for all");

        _tokenApprovals[tokenId] = to;
        emit Approval(owner, to, tokenId);
    }

    function getApproved(uint256 tokenId) public view returns (address) {
        require(_owners[tokenId] != address(0), "ERC721: approved query for nonexistent token");
        return _tokenApprovals[tokenId];
    }

    function setApprovalForAll(address operator, bool approved) public {
        _operatorApprovals[msg.sender][operator] = approved;
        emit ApprovalForAll(msg.sender, operator, approved);
    }

    function isApprovedForAll(address owner, address operator) public view returns (bool) {
        return _operatorApprovals[owner][operator];
    }

    function transferFrom(address from, address to, uint256 tokenId) public {
        address owner = ownerOf(tokenId);
        require(_isApprovedOrOwner(msg.sender, tokenId), "ERC721: transfer caller is not owner nor approved");
        _transfer(from, to, tokenId);
    }

    function safeTransferFrom(address from, address to, uint256 tokenId) public {
        transferFrom(from, to, tokenId);
    }

    function safeTransferFrom(address from, address to, uint256 tokenId, bytes memory _data) public {
        transferFrom(from, to, tokenId);
    }

    function _isApprovedOrOwner(address spender, uint256 tokenId) internal view returns (bool) {
        address owner = ownerOf(tokenId);
        return (spender == owner || getApproved(tokenId) == spender || isApprovedForAll(owner, spender));
    }

    function _transfer(address from, address to, uint256 tokenId) internal {
        require(ownerOf(tokenId) == from, "ERC721: transfer from incorrect owner");
        require(to != address(0), "ERC721: transfer to the zero address");

        // Clear approvals from the previous owner
        _tokenApprovals[tokenId] = address(0);
        emit Approval(from, address(0), tokenId);

        _balances[from] -= 1;
        _balances[to] += 1;
        _owners[tokenId] = to;

        emit Transfer(from, to, tokenId);
    }

    function claim() public {
        require(nextClaimId < MAX_SUPPLY, "All NFTs claimed");
        require(balanceOf(msg.sender) == 0, "Already claimed one"); 
        
        uint256 tokenId = nextClaimId;
        nextClaimId++;

        // Transfer from contract (which holds totalSupply) to user
        _transfer(address(this), msg.sender, tokenId);
    }

    function _mint(address to, uint256 tokenId) internal {
        require(to != address(0), "ERC721: mint to the zero address");
        
        _balances[to] += 1;
        _owners[tokenId] = to;

        emit Transfer(address(0), to, tokenId);
    }

    // ERC165 Logic
    function supportsInterface(bytes4 interfaceId) public view virtual returns (bool) {
        return
            interfaceId == 0x01ffc9a7 || // ERC165
            interfaceId == 0x80ac58cd;   // ERC721
    }
}
