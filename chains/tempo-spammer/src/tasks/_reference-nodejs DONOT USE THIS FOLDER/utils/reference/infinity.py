# ═══════════════════════════════════════════════════════════════════════════════
# INFINITY NAME MODULE - [15]
# ═══════════════════════════════════════════════════════════════════════════════

import asyncio
import random
import string
from web3 import Web3
from eth_account import Account
from config import CONFIG, INFINITY_NAME_CONTRACT, ERC20_ABI, COLORS, SYSTEM_CONTRACTS, FEE_MANAGER_ABI
from utils.helpers import async_sleep, short_hash, wait_for_tx_with_retry, countdown, get_random_int
from utils.wallet import get_private_keys

INFINITY_NAME_ABI = [
    {
        'constant': False,
        'inputs': [
            {'name': 'domain', 'type': 'string'},
            {'name': 'referrer', 'type': 'address'}
        ],
        'name': 'register',
        'outputs': [{'name': '', 'type': 'uint256'}],
        'type': 'function'
    },
    {
        'constant': True,
        'inputs': [{'name': 'domain', 'type': 'string'}],
        'name': 'isAvailable',
        'outputs': [{'name': '', 'type': 'bool'}],
        'type': 'function'
    },
    {
        'constant': True,
        'inputs': [],
        'name': 'price',
        'outputs': [{'name': '', 'type': 'uint256'}],
        'type': 'function'
    }
]

def generate_random_name(length=10):
    """Generate random domain name"""
    chars = string.ascii_lowercase + string.digits
    name = random.choice(string.ascii_lowercase)
    for i in range(1, length):
        name += random.choice(chars)
    return name

from utils.proxy import create_web3_with_proxy, get_proxy_for_key_index

async def run_infinity_name():
    """Main function of InfinityName module"""
    print('\n═══════════════════════════════════════════════════════════════')
    print('[15] 🌐 INFINITY NAME - Domain Registration')
    print('═══════════════════════════════════════════════════════════════\n')

    private_keys = get_private_keys()
    if len(private_keys) == 0:
        print('❌ Private keys not found in pv.txt')
        return

    wallets = [Account.from_key(pk) for pk in private_keys]

    print(f"\033[1m\033[36mFound {len(wallets)} wallet(s)\033[0m\n")

    # Убеждаемся, что все адреса в checksum формате
    infinity_contract_checksum = Web3.to_checksum_address(INFINITY_NAME_CONTRACT)
    path_usd_address_checksum = Web3.to_checksum_address(CONFIG['TOKENS']['PathUSD'])

    successful = 0
    failed = 0
    skipped = 0

    for w in range(len(wallets)):
        wallet = wallets[w]
        private_key = private_keys[w]
        
        # Setup Web3 with proxy for this wallet
        proxy = get_proxy_for_key_index(w)
        web3 = create_web3_with_proxy(CONFIG['RPC_URL'], proxy)
        
        # Recreate contract instances with new web3
        fee_manager = web3.eth.contract(address=Web3.to_checksum_address(SYSTEM_CONTRACTS['FEE_MANAGER']), abi=FEE_MANAGER_ABI)
        
        proxy_msg = f"Using Proxy: {proxy}" if proxy else "Using: Direct Connection"
        
        wallet_address = Web3.to_checksum_address(wallet.address)

        print(f"\n\033[1m\033[35mWALLET #{w + 1}/{len(wallets)}\033[0m")
        print(f"\033[1m\033[36mAddress: {wallet_address}\033[0m")
        print(f"\033[1m\033[36m{proxy_msg}\033[0m")
        print(f"\033[1m\033[36mContract: {infinity_contract_checksum}\033[0m")

        # Проверяем баланс fee токена
        try:
            current_fee_token = fee_manager.functions.userTokens(wallet_address).call()
            if current_fee_token == '0x0000000000000000000000000000000000000000':
                current_fee_token = CONFIG['TOKENS']['PathUSD']  # Default

            fee_token_contract = web3.eth.contract(address=Web3.to_checksum_address(current_fee_token), abi=ERC20_ABI)
            fee_balance = fee_token_contract.functions.balanceOf(wallet_address).call()
            fee_balance_formatted = fee_balance / (10 ** 6)

            # Минимальный баланс для оплаты газа (примерно 0.3 токена для регистрации домена)
            min_balance = int(0.3 * (10 ** 6))
            if fee_balance < min_balance:
                print(f"\033[1m\033[31m⚠️ Insufficient funds to pay gas!\033[0m")
                print(f"\033[1m\033[33m  Fee token balance: {fee_balance_formatted}\033[0m")
                print(f"\033[1m\033[33m  Minimum required: 0.3 tokens\033[0m")
                skipped += 1
                continue
        except Exception as fee_check_err:
            print(f"\033[1m\033[33m⚠️ Failed to check fee token balance: {str(fee_check_err)[:50]}\033[0m")
            # Continue execution, but user will be warned

        try:
            infinity_name = web3.eth.contract(address=infinity_contract_checksum, abi=INFINITY_NAME_ABI)
            path_usd_contract = web3.eth.contract(address=path_usd_address_checksum, abi=ERC20_ABI)

            # Check PathUSD balance
            path_balance = path_usd_contract.functions.balanceOf(wallet_address).call()
            print(f"\033[1m\033[36m💰 PathUSD Balance: {path_balance / (10 ** 6)}\033[0m")

            # Generate random name for each wallet
            domain_name = generate_random_name(10)
            print(f"\033[1m\033[32m🌐 Domain to register: {domain_name}.tempo\033[0m")

            # Check availability (may not work)
            try:
                available = infinity_name.functions.isAvailable(domain_name).call()
                print(f"\033[1m\033[32m✓ Available: {available}\033[0m")
            except Exception:
                print('\033[1m\033[33m⚠️ Failed to check availability\033[0m')

            # Approve PathUSD with retry
            approve_amount = int(1000 * (10 ** 6))
            allowance = path_usd_contract.functions.allowance(wallet_address, infinity_contract_checksum).call()

            if allowance < approve_amount:
                print('\033[1m\033[36m📝 Approving PathUSD...\033[0m')
                for retry in range(3):
                    try:
                        nonce = web3.eth.get_transaction_count(wallet_address)
                        max_uint256 = 2**256 - 1
                        approve_tx = path_usd_contract.functions.approve(infinity_contract_checksum, max_uint256).build_transaction({
                            'from': wallet_address,
                            'nonce': nonce,
                            'gas': 100000,
                            'gasPrice': web3.eth.gas_price,
                            'chainId': CONFIG['CHAIN_ID']
                        })
                        # Sign transaction
                        try:
                            signed_approve = wallet.sign_transaction(approve_tx)
                            raw_tx = signed_approve.rawTransaction
                        except (AttributeError, TypeError):
                            signed_approve = Account.sign_transaction(approve_tx, private_key)
                            raw_tx = signed_approve.rawTransaction if hasattr(signed_approve, 'rawTransaction') else signed_approve.raw_transaction
                        await wait_for_tx_with_retry(web3, web3.eth.send_raw_transaction(raw_tx).hex())
                        print('\033[1m\033[32m✓ Approved\033[0m')
                        break
                    except Exception as e:
                        if '502' in str(e) and retry < 2:
                            print(f"\033[1m\033[33m⚠️ RPC error, retry {retry + 1}/3...\033[0m")
                            await async_sleep(10)
                        else:
                            raise e

            # Register domain with retry
            print(f"\033[1m\033[36m🚀 Registering {domain_name}.tempo...\033[0m")
            registered = False
            for retry in range(3):
                if registered:
                    break
                try:
                    nonce = web3.eth.get_transaction_count(wallet_address)
                    tx = infinity_name.functions.register(domain_name, '0x0000000000000000000000000000000000000000').build_transaction({
                        'from': wallet_address,
                        'nonce': nonce,
                        'gas': 500000,
                        'gasPrice': web3.eth.gas_price,
                        'chainId': CONFIG['CHAIN_ID']
                    })

                    # Подписываем транзакцию
                    try:
                        signed_tx = wallet.sign_transaction(tx)
                        raw_tx = signed_tx.rawTransaction
                    except (AttributeError, TypeError):
                        signed_tx = Account.sign_transaction(tx, private_key)
                        raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
                    tx_hash = web3.eth.send_raw_transaction(raw_tx)
                    print(f"\033[1m\033[33mTX: {short_hash(tx_hash.hex())}\033[0m")

                    receipt = await wait_for_tx_with_retry(web3, tx_hash.hex())

                    if receipt['status'] == 1:
                        print(f"\033[1m\033[32m✅ Domain registered!\033[0m")
                        print(f"\033[1m\033[36m🌐 Domain: {domain_name}.tempo\033[0m")
                        print(f"\033[1m\033[36m📦 Block: {receipt['blockNumber']}\033[0m")
                        successful += 1
                        registered = True
                    else:
                        print('\033[1m\033[31m❌ TX reverted\033[0m')
                        failed += 1
                        registered = True
                except Exception as e:
                    err_msg = str(e)
                    if ('502' in err_msg or '503' in err_msg) and retry < 2:
                        print(f"\033[1m\033[33m⚠️ RPC error, retry {retry + 1}/3...\033[0m")
                        await async_sleep(10)
                    else:
                        if 'execution reverted' in err_msg or 'CALL_EXCEPTION' in err_msg:
                            print('\033[1m\033[33m⚠️ Contract unavailable or domain taken\033[0m')
                        else:
                            print(f"\033[1m\033[31m✗ Ошибка: {err_msg[:100]}\033[0m")
                        failed += 1
                        registered = True

        except Exception as error:
            err_msg = str(error)
            if 'execution reverted' in err_msg or 'CALL_EXCEPTION' in err_msg:
                print('\033[1m\033[33m⚠️ Contract unavailable or domain taken\033[0m')
            else:
                print(f"\033[1m\033[31m✗ Ошибка: {err_msg[:100]}\033[0m")
            failed += 1

        if w < len(wallets) - 1:
            await countdown(get_random_int(5, 10), 'Next wallet in')

    # Final statistics
    print(f"\n\033[1m\033[35m📊  DOMAIN REGISTRATION RESULTS\033[0m")
    print(f"\033[1m\033[32m✓\033[0m Successful: \033[1m\033[32m{successful}\033[0m")
    print(f"\033[1m\033[31m✗\033[0m Failed: \033[1m\033[31m{failed}\033[0m")
    if skipped > 0:
        print(f"\033[1m\033[33m⊘\033[0m Skipped: \033[1m\033[33m{skipped}\033[0m")
    print(f"\033[1m\033[36m◆\033[0m Total wallets: \033[1m\033[36m{len(wallets)}\033[0m")
