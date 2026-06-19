# ═══════════════════════════════════════════════════════════════════════════════
# ANALYTICS MODULE - [19]
# ═══════════════════════════════════════════════════════════════════════════════

import asyncio
from web3 import Web3
from eth_account import Account
from config import CONFIG, SYSTEM_CONTRACTS, ERC20_ABI, FEE_MANAGER_ABI, COLORS
from utils.helpers import async_sleep
from utils.wallet import get_private_keys
from utils.proxy import create_web3_with_proxy, get_proxy_for_key_index

async def run_analytics():
    """Main entry for analytics module"""
    BOLD_MAGENTA = COLORS.BOLD_MAGENTA
    RESET = COLORS.RESET
    BOLD_CYAN = COLORS.BOLD_CYAN

    print(f"\n  {BOLD_MAGENTA}📊  ANALYTICS & REPORTING{RESET}\n")

    try:
        private_keys = get_private_keys()
        if len(private_keys) == 0:
            print('\033[1m\033[31mPrivate keys not found in pv.txt\033[0m')
            return

        wallets = [Account.from_key(pk) for pk in private_keys]

        print(f"\033[1m\033[36mAnalyzing {len(wallets)} wallet(s)...\033[0m\n")

        for w in range(len(wallets)):
            wallet = wallets[w]
            
            # Setup Web3 with proxy for this wallet
            proxy = get_proxy_for_key_index(w)
            web3 = create_web3_with_proxy(CONFIG['RPC_URL'], proxy)
            proxy_msg = f"Using Proxy: {proxy}" if proxy else "Direct Connection"

            print(f"\n╔════════════════════════════════════════════════════════════════╗")
            print(f"║  WALLET #{w + 1}: {wallet.address[:10]}...{wallet.address[-8]}  ║")
            print(f"║  {proxy_msg.center(62)}  ║")
            print(f"╠════════════════════════════════════════════════════════════════╣")

            # Token balances
            print(f"║  {BOLD_CYAN}TOKENS:{RESET}                                                    ║")
            for symbol, address in CONFIG['TOKENS'].items():
                try:
                    contract = web3.eth.contract(address=Web3.to_checksum_address(address), abi=ERC20_ABI)
                    balance = contract.functions.balanceOf(wallet.address).call()
                    formatted = balance / (10 ** 6)
                    display = f"{formatted:.2f}"
                    print(f"║  {symbol.ljust(12)} │ {display.rjust(15)} {symbol.ljust(8)}  ║")
                except Exception:
                    print(f"║  {symbol.ljust(12)} │ {' ERROR'.rjust(15)}                ║")

            # LP balances
            print(f"║                                                                ║")
            print(f"║  {BOLD_CYAN}LP POSITIONS:{RESET}                                              ║")

            fee_manager = web3.eth.contract(address=Web3.to_checksum_address(SYSTEM_CONTRACTS['FEE_MANAGER']), abi=FEE_MANAGER_ABI)
            pairs = [
                ['AlphaUSD', 'PathUSD'],
                ['BetaUSD', 'PathUSD'],
                ['ThetaUSD', 'PathUSD']
            ]

            for token1, token2 in pairs:
                try:
                    pool_id = fee_manager.functions.getPoolId(
                        Web3.to_checksum_address(CONFIG['TOKENS'][token1]),
                        Web3.to_checksum_address(CONFIG['TOKENS'][token2])
                    ).call()
                    lp_balance = fee_manager.functions.liquidityBalances(pool_id, wallet.address).call()
                    formatted = lp_balance / (10 ** 6)
                    display = f"{formatted:.2f}"
                    pair_name = f"{token1}/{token2}"
                    print(f"║  {pair_name.ljust(20)} │ {display.rjust(15)} LP       ║")
                except Exception:
                    pass

            print(f"╚════════════════════════════════════════════════════════════════╝")

            if w < len(wallets) - 1:
                await async_sleep(1)

        print(f"\n\033[1m\033[32m✓ Analytics completed\033[0m")

    except Exception as error:
        print(f"\033[1m\033[31mAnalytics Error: {error}\033[0m")
