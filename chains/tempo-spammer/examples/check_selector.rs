use alloy::primitives::{hex, keccak256};

fn main() {
    let errors = vec![
        "AccessControlUnauthorizedAccount(address,bytes32)",
        "OwnableUnauthorizedAccount(address)",
        "InvalidSignature()",
        "NonceExpired()",
        "InsufficientBalance()",
        "ExecutionReverted()",
        "InvalidInitialization()",
        "NotInitializing()",
        "InvalidTransferPolicyId()",
        "InvalidQuoteToken()",
        "InvalidSupplyCap()",
        "SupplyCapExceeded()",
        "ContractPaused()",
        "InvalidRecipient()",
        "PolicyForbids()",
        "InsufficientAllowance()",
        "InsufficientBalance(uint256,uint256,address)",
        "ProtectedAddress()",
        "InvalidAmount()",
        "NoOptedInSupply()",
        "AccessControlUnauthorizedAccount(address,bytes32)",
        "OwnableUnauthorizedAccount(address)",
        "InvalidSignature()",
        "NonceExpired()",
        "hasRole(bytes32,address)",
        "hasRole(address,bytes32)",
        "grantRole(bytes32,address)",
        "SelectorNotFound(bytes4)",
        "FunctionNotFound(bytes4)",
        "InvalidSelector(bytes4)",
        "UnknownSelector(bytes4)",
        "DispatchError(bytes4)",
        "InvalidFunction(bytes4)",
        "FunctionNotImplemented(bytes4)",
    ];

    println!("Target: 0xaa4bc69a");
    for err in errors {
        let hash = keccak256(err.as_bytes());
        let selector = &hash[0..4];
        println!("{} -> 0x{}", err, hex::encode(selector));
        if hex::encode(selector) == "aa4bc69a" {
            println!("MATCH FOUND: {}", err);
        }
    }
}
