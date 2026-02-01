import { ethers } from 'ethers';

const errors = [
    "AccessControlUnauthorizedAccount(address,bytes32)",
    "AccessControlBadConfirmation()",
    "OwnableUnauthorizedAccount(address)",
    "EnforcedPause()",
    "ExpectedPause()",
    "CapExceeded()",
    "InvalidAmount()",
    "InvalidRecipient()",
    "ERC20InsufficientBalance(address,uint256,uint256)",
    "ERC20InvalidReceiver(address)",
    "CallerNotIssuer(address)",
    "NotIssuer(address)",
    "InvalidIssuer(address)",
    "IssuerRoleRequired(address)",
    "MustHaveIssuerRole(address)",
    "Unauthorized(address)",
    "Forbidden(address)",
    "AccountMissingRole(address,bytes32)",
    "AccessControlUnauthorizedAccount(address,bytes32)",
    "CallerIsNotIssuer(address)",
    "SenderNotIssuer(address)",
    "NotIssuer()",
    "CallerNotIssuer()",
    "InvalidAmount(uint256)",
    "TokenMintingDisabled()",
    "MaxSupplyExceeded()",
    "CallerNotMinter(address)",
    "MinterRoleRequired(address)",
    "NotMinter(address)",
    "InvalidMinter(address)",
    "GeneralError(string)",
    "ExecutionFailed()",
    "UnauthorizedMinter(address)",
    "AccessDenied(address)",
    "PermissionsMissing(address)",
    "RoleMissing(bytes32,address)",
    "MissingRole(address,bytes32)",
    "NotAuthorized()",
    "OnlyIssuer()",
    "OnlyMinter()"
];

console.log("Checking Error Selectors:");
errors.forEach(err => {
    const id = ethers.id(err).substring(0, 10);
    console.log(`${id} : ${err}`);
});
