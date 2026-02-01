// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
}

/**
 * @title ViralFaucet
 * @dev A faucet that allows anyone to claim small amounts of ERC20 tokens.
 * Supports funding with any ERC20 stablecoin.
 */
contract ViralFaucet {
    mapping(address => mapping(address => uint256)) public lastClaim; // user -> token -> timestamp
    uint256 public constant COOLDOWN = 1 minutes;
    
    event Claimed(address indexed user, address indexed token, uint256 amount);
    event Funded(address indexed funder, address indexed token, uint256 amount);

    function fund(address token, uint256 amount) external {
        require(amount > 0, "Amount must be > 0");
        bool success = IERC20(token).transferFrom(msg.sender, address(this), amount);
        require(success, "Transfer failed");
        emit Funded(msg.sender, token, amount);
    }

    function claim(address token, uint256 amount) external {
        require(block.timestamp >= lastClaim[msg.sender][token] + COOLDOWN, "Cooldown active for this token");
        
        uint256 balance = IERC20(token).balanceOf(address(this));
        require(balance >= amount, "Faucet empty for this token");

        lastClaim[msg.sender][token] = block.timestamp;
        
        bool success = IERC20(token).transfer(msg.sender, amount);
        require(success, "Transfer failed");

        emit Claimed(msg.sender, token, amount);
    }

    function getBalance(address token) external view returns (uint256) {
        return IERC20(token).balanceOf(address(this));
    }
}
