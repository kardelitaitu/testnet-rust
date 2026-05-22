# Overlayer Tasks — Sepolia-OverLayer

| ID | Name | Description |
|----|------|-------------|
| t01 | checkBalance | Check ETH, USDT, USDC, USDT+ (T+), and USDC+ (C+) balances |
| t02 | mintUsdtPlus | Mint USDT+ (T+) using 80% of USDT balance |
| t03 | mintUsdcPlus | Mint USDC+ (C+) using 80% of USDC balance |
| t04 | redeemUsdtPlus | Redeem USDT+ (burn T+) for USDT via `redeem(tuple)` (3% of T+) |
| t05 | redeemUsdcPlus | Redeem USDC+ (burn C+) for USDC via `redeem(tuple)` (3% of C+) |
| t06 | stakeUsdtPlus | Stake USDT+ (T+) into staking contract via `deposit(uint256,address)` (5% of T+, unlimited approve if needed) |
| t07 | stakeUsdcPlus | Stake USDC+ (C+) into C+ vault via `deposit(uint256,address)` (5% of C+, unlimited approve if needed) |
| t08 | unstakeTplus | Unstake sOverl... (shares) for T+ via `redeem(uint256,address,address)` (5% of sOverl..., min 0.01) |
| t09 | unstakeCplus | Unstake sOverl... (shares) for C+ via `redeem(uint256,address,address)` (5% of sOverl..., min 0.01) |
| t10 | aaveUsdtFaucet | Mint 10,000 USDT from AAVE faucet via `mint(address,address,uint256)` |
| t11 | aaveUsdcFaucet | Mint 10,000 USDC from AAVE faucet via `mint(address,address,uint256)` |
| t12 | bridgeTplus | Bridge T+ (USDT+) to Base Sepolia via LayerZero OFT `send()` (10% of T+, 0.0002 ETH fee) |
| t13 | bridgeCplus | Bridge C+ (USDC+) to Base Sepolia via LayerZero OFT `send()` (10% of C+, 0.0002 ETH fee) |
| t14 | sendRandomUsdtPlus | Send 0.5% of USDT+ (T+) balance to a random EVM address via `transfer(address,uint256)` |
| t15 | sendRandomUsdcPlus | Send 0.5% of USDC+ (C+) balance to a random EVM address via `transfer(address,uint256)` |
| t16 | bridgeBackTplus | Bridge T+ (USDT+) from Base Sepolia → Eth Sepolia via LayerZero OFT `send()` (40% of T+, ~0.0001 ETH fee) |
| t17 | bridgeBackCplus | Bridge C+ (USDC+) from Base Sepolia → Eth Sepolia via LayerZero OFT `send()` (40% of C+, ~0.0001 ETH fee) |
| t18 | receiveTplus | Send 5% of T+ + 0.0000721 ETH to a random ephemeral proxy wallet, proxy returns all T+ back to main — tests ERC-20 send/receive round-trip |
| t19 | receiveCplus | Send 5% of C+ + 0.0000721 ETH to a random ephemeral proxy wallet, proxy returns all C+ back to main — tests ERC-20 send/receive round-trip |
