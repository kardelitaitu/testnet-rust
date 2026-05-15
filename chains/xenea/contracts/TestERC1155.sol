// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TestERC1155 {
    mapping(uint256 => mapping(address => uint256)) public balanceOf;
    mapping(address => mapping(address => bool)) public isApprovedForAll;

    event TransferSingle(address indexed operator, address indexed from, address indexed to, uint256 id, uint256 value);
    event TransferBatch(address indexed operator, address indexed from, address indexed to, uint256[] ids, uint256[] values);
    event ApprovalForAll(address indexed account, address indexed operator, bool approved);

    function mint(address to, uint256 id, uint256 amount, bytes memory data) public {
        balanceOf[id][to] += amount;
        emit TransferSingle(msg.sender, address(0), to, id, amount);
    }

    function mintBatch(address to, uint256[] memory ids, uint256[] memory amounts, bytes memory data) public {
        require(ids.length == amounts.length, "Length mismatch");
        for (uint256 i = 0; i < ids.length; ++i) {
            balanceOf[ids[i]][to] += amounts[i];
        }
        emit TransferBatch(msg.sender, address(0), to, ids, amounts);
    }

    function safeTransferFrom(address from, address to, uint256 id, uint256 amount, bytes memory data) public {
        require(from == msg.sender || isApprovedForAll[from][msg.sender], "Not authorized");
        require(balanceOf[id][from] >= amount, "Insufficient balance");
        
        balanceOf[id][from] -= amount;
        balanceOf[id][to] += amount;
        
        emit TransferSingle(msg.sender, from, to, id, amount);
    }

    function safeBatchTransferFrom(address from, address to, uint256[] memory ids, uint256[] memory amounts, bytes memory data) public {
        require(from == msg.sender || isApprovedForAll[from][msg.sender], "Not authorized");
        require(ids.length == amounts.length, "Length mismatch");

        for (uint256 i = 0; i < ids.length; ++i) {
            uint256 id = ids[i];
            uint256 amount = amounts[i];
            require(balanceOf[id][from] >= amount, "Insufficient balance");
            balanceOf[id][from] -= amount;
            balanceOf[id][to] += amount;
        }

        emit TransferBatch(msg.sender, from, to, ids, amounts);
    }

    function setApprovalForAll(address operator, bool approved) public {
        isApprovedForAll[msg.sender][operator] = approved;
        emit ApprovalForAll(msg.sender, operator, approved);
    }
}
