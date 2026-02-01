// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title TempoSplitter
 * @dev This contract allows to split Ether payments among a group of accounts. The split can be in equal parts or in any other arbitrary proportion.
 * The way this is specified is by assigning each account to a number of shares.
 * Of all the Ether that this contract receives, each account will then be able to claim an amount proportional to the percentage of total shares they were assigned.
 * The contract tracks 'totalReceived' lifetime accumulation to ensure mathematical correctness over time (dust handling).
 */
contract TempoSplitter {
    event PayeeAdded(address account, uint256 shares, string memo);
    event PaymentReleased(address to, uint256 amount, string memo);
    event ERC20PaymentReleased(address indexed token, address to, uint256 amount, string memo);
    event PaymentReceived(address from, uint256 amount);

    uint256 private _totalShares;
    uint256 private _totalReleased;

    mapping(address => uint256) private _shares;
    mapping(address => uint256) private _released;
    mapping(address => string) private _memos;
    address[] private _payees;

    mapping(address => uint256) private _erc20TotalReleased;
    mapping(address => mapping(address => uint256)) private _erc20Released;

    /**
     * @dev Creates an instance of `TempoSplitter` where each account in `payees` is assigned the number of shares at
     * the matching position in the `shares` array.
     * All addresses in `payees` must be non-zero. Both arrays must have the same non-zero length, and there must be no
     * duplicates in `payees`.
     */
    constructor(address[] memory payees, uint256[] memory shares_, string[] memory memos_) payable {
        require(payees.length == shares_.length, "TempoSplitter: payees and shares length mismatch");
        require(payees.length == memos_.length, "TempoSplitter: payees and memos length mismatch");
        require(payees.length > 0, "TempoSplitter: no payees");

        for (uint256 i = 0; i < payees.length; i++) {
            _addPayee(payees[i], shares_[i], memos_[i]);
        }
    }

    /**
     * @dev The Ether received will be logged with {PaymentReceived} events. Note that these events are not fully
     * reliable: it's possible for a contract to receive Ether without triggering this function./
     * This function calls distribute() for native currency automatically if gas allows, but we keep it passive to save gas for sender.
     */
    receive() external payable {
        emit PaymentReceived(msg.sender, msg.value);
    }

    /**
     * @dev Getter for the total shares held by payees.
     */
    function totalShares() public view returns (uint256) {
        return _totalShares;
    }

    /**
     * @dev Getter for the total amount of Ether already released.
     */
    function totalReleased() public view returns (uint256) {
        return _totalReleased;
    }

    /**
     * @dev Getter for the total amount of `token` already released. `token` should be the address of an IERC20 contract.
     */
    function totalReleased(address token) public view returns (uint256) {
        return _erc20TotalReleased[token];
    }

    /**
     * @dev Getter for the amount of shares held by an account.
     */
    function shares(address account) public view returns (uint256) {
        return _shares[account];
    }

    /**
     * @dev Getter for the memo associated with an account.
     */
    function memo(address account) public view returns (string memory) {
        return _memos[account];
    }

    /**
     * @dev Getter for the amount of Ether already released to a payee.
     */
    function released(address account) public view returns (uint256) {
        return _released[account];
    }

    /**
     * @dev Getter for the amount of `token` already released to a payee. `token` should be the address of an IERC20 contract.
     */
    function released(address token, address account) public view returns (uint256) {
        return _erc20Released[token][account];
    }

    /**
     * @dev Getter for the address of the payee number `index`.
     */
    function payee(uint256 index) public view returns (address) {
        return _payees[index];
    }

    /**
     * @dev Getter for the length of the payee array.
     */
    function payeeCount() public view returns (uint256) {
        return _payees.length;
    }

    /**
     * @dev Distributes the Native Ether of the contract to all payees.
     */
    function distributeNative() public {
        uint256 totalReceived = address(this).balance + _totalReleased;
        
        for (uint256 i = 0; i < _payees.length; i++) {
            address account = _payees[i];
            uint256 payment = (totalReceived * _shares[account]) / _totalShares - _released[account];

            if (payment > 0) {
                _released[account] += payment;
                _totalReleased += payment;
                (bool success, ) = account.call{value: payment}("");
                require(success, "TempoSplitter: unable to send value, recipient may have reverted");
                emit PaymentReleased(account, payment, _memos[account]);
            }
        }
    }

    /**
     * @dev Distributes the `token` balance of the contract to all payees.
     * @param token Address of the IERC20 token contract.
     */
    function distribute(address token) public {
        if (token == address(0)) {
            distributeNative();
            return;
        }

        IERC20 erc20 = IERC20(token);
        uint256 balance = erc20.balanceOf(address(this));
        uint256 totalReceived = balance + _erc20TotalReleased[token];

        for (uint256 i = 0; i < _payees.length; i++) {
            address account = _payees[i];
            uint256 payment = (totalReceived * _shares[account]) / _totalShares - _erc20Released[token][account];

            if (payment > 0) {
                _erc20Released[token][account] += payment;
                _erc20TotalReleased[token] += payment;
                bool success = erc20.transfer(account, payment);
                require(success, "TempoSplitter: ERC20 transfer failed");
                emit ERC20PaymentReleased(token, account, payment, _memos[account]);
            }
        }
    }

    /**
     * @dev Add a new payee to the contract.
     * @param account The address of the payee to add.
     * @param shares_ The number of shares owned by the payee.
     * @param memo_ The memo for the payee.
     */
    function _addPayee(address account, uint256 shares_, string memory memo_) private {
        require(account != address(0), "TempoSplitter: account is the zero address");
        require(shares_ > 0, "TempoSplitter: shares are 0");
        require(_shares[account] == 0, "TempoSplitter: account already has shares");

        _payees.push(account);
        _shares[account] = shares_;
        _memos[account] = memo_;
        _totalShares = _totalShares + shares_;
        emit PayeeAdded(account, shares_, memo_);
    }
}

interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
}
