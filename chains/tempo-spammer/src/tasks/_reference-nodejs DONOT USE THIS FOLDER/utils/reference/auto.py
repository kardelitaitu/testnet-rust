# ═══════════════════════════════════════════════════════════════════════════════
# AUTO MODE MODULE - [21]
# ═══════════════════════════════════════════════════════════════════════════════

import asyncio
import random
import string
import re
import time  # Added for sync sleep
from web3 import Web3
from eth_account import Account
from solcx import compile_source, set_solc_version
from config import CONFIG, SYSTEM_CONTRACTS, RETRIEVER_NFT_CONTRACT, ERC20_ABI, TIP20_FACTORY_ABI, STABLECOIN_DEX_ABI, FEE_MANAGER_ABI, COLORS
from utils.helpers import async_sleep, short_hash, wait_for_tx_with_retry, ask_question, sync_wait_for_tx_with_retry
from utils.wallet import load_created_tokens
from utils.wallet import get_private_keys

TIP20_MINT_ABI = [
    {
        'constant': False,
        'inputs': [
            {'name': 'to', 'type': 'address'},
            {'name': 'amount', 'type': 'uint256'}
        ],
        'name': 'mint',
        'outputs': [],
        'type': 'function'
    },
    {
        'constant': False,
        'inputs': [{'name': 'amount', 'type': 'uint256'}],
        'name': 'burn',
        'outputs': [],
        'type': 'function'
    },
    {
        'constant': True,
        'inputs': [
            {'name': 'role', 'type': 'bytes32'},
            {'name': 'account', 'type': 'address'}
        ],
        'name': 'hasRole',
        'outputs': [{'name': '', 'type': 'bool'}],
        'type': 'function'
    },
    {
        'constant': False,
        'inputs': [
            {'name': 'role', 'type': 'bytes32'},
            {'name': 'account', 'type': 'address'}
        ],
        'name': 'grantRole',
        'outputs': [],
        'type': 'function'
    },
    {
        'constant': True,
        'inputs': [{'name': '_owner', 'type': 'address'}],
        'name': 'balanceOf',
        'outputs': [{'name': 'balance', 'type': 'uint256'}],
        'type': 'function'
    },
    {
        'constant': False,
        'inputs': [
            {'name': 'to', 'type': 'address'},
            {'name': 'amount', 'type': 'uint256'},
            {'name': 'memo', 'type': 'bytes32'}
        ],
        'name': 'transferWithMemo',
        'outputs': [],
        'type': 'function'
    }
]

RETRIEVER_NFT_ABI = [
    {
        'constant': False,
        'inputs': [
            {'name': '_receiver', 'type': 'address'},
            {'name': '_quantity', 'type': 'uint256'},
            {'name': '_currency', 'type': 'address'},
            {'name': '_pricePerToken', 'type': 'uint256'},
            {
                'name': '_allowlistProof',
                'type': 'tuple',
                'components': [
                    {'name': 'proof', 'type': 'bytes32[]'},
                    {'name': 'quantityLimitPerWallet', 'type': 'uint256'},
                    {'name': 'pricePerToken', 'type': 'uint256'},
                    {'name': 'currency', 'type': 'address'}
                ]
            },
            {'name': '_data', 'type': 'bytes'}
        ],
        'name': 'claim',
        'outputs': [],
        'type': 'function'
    },
    {
        'constant': True,
        'inputs': [{'name': 'owner', 'type': 'address'}],
        'name': 'balanceOf',
        'outputs': [{'name': '', 'type': 'uint256'}],
        'type': 'function'
    }
]

def shuffle_array(array):
    """Shuffle a list and return a new one"""
    arr = array.copy()
    random.shuffle(arr)
    return arr

def parse_wallet_selection(input_str, total_wallets):
    """Parse wallet selection string into indices"""
    selected = set()

    # Remove spaces
    input_str = input_str.strip().replace(' ', '')

    # Split by comma
    parts = input_str.split(',')

    for part in parts:
        if '-' in part:
            # Range: "1-10"
            start, end = map(int, part.split('-'))

            if start < 1 or end > total_wallets or start > end:
                print(f"\033[1m\033[31m⚠️ Range out of bounds: {part} (available 1-{total_wallets})\033[0m")
                continue

            for i in range(start, end + 1):
                selected.add(i)
        else:
            # Single number: "5"
            try:
                num = int(part)
                if num < 1 or num > total_wallets:
                    print(f"\033[1m\033[31m⚠️ Index out of bounds: {num} (available 1-{total_wallets})\033[0m")
                    continue
                selected.add(num)
            except ValueError:
                print(f"\033[1m\033[31m⚠️ Invalid number: {part}\033[0m")

    # Convert to sorted zero-based indices
    return sorted([n - 1 for n in selected])

# ACTIVITIES

def activity1_deploy(web3, wallet):
    """Deploy a simple contract (Synchronous)"""
    source = 'pragma solidity ^0.8.20; contract TestContract { string public message = "Hello Tempo!"; }'
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        set_solc_version('0.8.20')
        compiled = compile_source(source, solc_version='0.8.20', optimize=True, optimize_runs=200)
        contract_interface = compiled['<stdin>:TestContract']
        bytecode = '0x' + contract_interface['bin']

        contract = web3.eth.contract(abi=contract_interface['abi'], bytecode=bytecode)
        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        transaction = contract.constructor().build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 2500000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })

        # Подписываем транзакцию
        # Sign transaction
        signed_txn = wallet.sign_transaction(transaction)
        raw_tx = signed_txn.rawTransaction if hasattr(signed_txn, 'rawTransaction') else signed_txn.raw_transaction

        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        receipt = sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        addr = receipt['contractAddress']
        print(f"  → Contract: {addr}")
        print(f"  → TX: {short_hash(tx_hash.hex())}")
        return tx_hash.hex()
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity2_faucet(web3, wallet, max_retries=3):
    """Call faucet with retry logic (Synchronous)"""
    for attempt in range(max_retries):
        try:
            print(f"  → Calling faucet (attempt {attempt+1}/{max_retries})...")
            tx_hashes = web3.manager.request_blocking('tempo_fundAddress', [wallet.address])
            print(f"  → Received: 4 tokens of 1,000,000")
            time.sleep(2)
            return 'faucet'
        except Exception as e:
            error_str = str(e).lower()
            
            # Check if it's a retryable error
            is_retryable = any(x in error_str for x in [
                'timeout', 'connection', 'refused', '502', '503', 
                'bad gateway', 'timed out', 'read timed out'
            ])
            
            if attempt < max_retries - 1 and is_retryable:
                wait_time = (attempt + 1) * 5  # 5s, 10s, 15s
                print(f"  → Faucet error (retryable): {str(e)[:60]}")
                time.sleep(wait_time)
                continue
            else:
                print(f"  → Faucet Error: {str(e)[:60]}")
                return None
    
    return None

def activity3_send_tokens(web3, wallet):
    """Send small token transfer (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        path_usd = web3.eth.contract(address=Web3.to_checksum_address(CONFIG['TOKENS']['PathUSD']), abi=ERC20_ABI)
        random_address = Web3.to_checksum_address(Account.create().address)
        amount = int(0.01 * (10 ** 6))
        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        tx = path_usd.functions.transfer(random_address, amount).build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 100000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })
        # Sign transaction
        signed_tx = wallet.sign_transaction(tx)
        raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
        
        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        print(f"  → 0.01 PathUSD → {short_hash(random_address)}")
        print(f"  → TX: {short_hash(tx_hash.hex())}")
        return tx_hash.hex()
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity4_create_stablecoin(web3, wallet):
    """Create a new stablecoin via factory (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        factory_address = Web3.to_checksum_address(SYSTEM_CONTRACTS['TIP20_FACTORY'])
        factory = web3.eth.contract(address=factory_address, abi=TIP20_FACTORY_ABI)
        random_suffix = ''.join(random.choices(string.ascii_uppercase + string.digits, k=4))
        token_name = f'Test Token {random_suffix}'
        token_symbol = f'T{random_suffix}USD'

        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        tx = factory.functions.createToken(token_name, token_symbol, 'USD', Web3.to_checksum_address(CONFIG['TOKENS']['PathUSD']), wallet_address).build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 500000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })
        # Sign transaction
        signed_tx = wallet.sign_transaction(tx)
        raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
        
        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        receipt = sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        print(f"  → TX: {short_hash(tx_hash.hex())}")

        # Parse TokenCreated event
        factory_contract = web3.eth.contract(address=factory_address, abi=TIP20_FACTORY_ABI)
        for log in receipt.get('logs', []):
            try:
                event = factory_contract.events.TokenCreated().process_log(log)
                token_address = Web3.to_checksum_address(event['args']['token'])
                print(f"  → Token: {token_address}")
                return token_address
            except:
                pass
        return None
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity5_swap(web3, wallet):
    """Swap tokens on DEX (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        dex_address = Web3.to_checksum_address(SYSTEM_CONTRACTS['STABLECOIN_DEX'])
        dex = web3.eth.contract(address=dex_address, abi=STABLECOIN_DEX_ABI)
        path_usd_address = Web3.to_checksum_address(CONFIG['TOKENS']['PathUSD'])
        path_usd = web3.eth.contract(address=path_usd_address, abi=ERC20_ABI)
        amount = int(1 * (10 ** 6))

        allowance = path_usd.functions.allowance(wallet_address, dex_address).call()
        if allowance < amount:
            nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
            max_uint256 = 2**256 - 1
            approve_tx = path_usd.functions.approve(dex_address, max_uint256).build_transaction({
                'from': wallet_address,
                'nonce': nonce,
                'gas': 100000,
                'gasPrice': web3.eth.gas_price,
                'chainId': CONFIG['CHAIN_ID']
            })
            # Sign transaction
            signed_approve = wallet.sign_transaction(approve_tx)
            raw_tx = signed_approve.rawTransaction if hasattr(signed_approve, 'rawTransaction') else signed_approve.raw_transaction
            
            approve_hash = web3.eth.send_raw_transaction(raw_tx)
            sync_wait_for_tx_with_retry(web3, approve_hash.hex())
            print(f"  → Approve TX: {short_hash(approve_hash.hex())}")

        alpha_usd_address = Web3.to_checksum_address(CONFIG['TOKENS']['AlphaUSD'])
        quote = dex.functions.quoteSwapExactAmountIn(path_usd_address, alpha_usd_address, amount).call()

        if quote > 0:
            min_out = (quote * 99) // 100
            nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
            tx = dex.functions.swapExactAmountIn(path_usd_address, alpha_usd_address, amount, min_out).build_transaction({
                'from': wallet_address,
                'nonce': nonce,
                'gas': 300000,
                'gasPrice': web3.eth.gas_price,
                'chainId': CONFIG['CHAIN_ID']
            })
            # Sign transaction
            signed_swap = wallet.sign_transaction(tx)
            raw_tx = signed_swap.rawTransaction if hasattr(signed_swap, 'rawTransaction') else signed_swap.raw_transaction
            
            tx_hash = web3.eth.send_raw_transaction(raw_tx)
            sync_wait_for_tx_with_retry(web3, tx_hash.hex())
            print(f"  → 1 PathUSD → AlphaUSD")
            print(f"  → Swap TX: {short_hash(tx_hash.hex())}")
            return tx_hash.hex()
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity6_add_liquidity(web3, wallet):
    """Add liquidity to fee manager pool with random tokens and amounts (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        fee_manager_address = Web3.to_checksum_address(SYSTEM_CONTRACTS['FEE_MANAGER'])
        fee_manager = web3.eth.contract(address=fee_manager_address, abi=FEE_MANAGER_ABI)
        
        # Randomly select 2 different tokens
        token_names = list(CONFIG['TOKENS'].keys())
        random.shuffle(token_names)
        token1_name = token_names[0]
        token2_name = token_names[1]
        
        token1_address = Web3.to_checksum_address(CONFIG['TOKENS'][token1_name])
        token2_address = Web3.to_checksum_address(CONFIG['TOKENS'][token2_name])
        
        # Random amounts: 5-10 USD for validator token (6 decimals)
        amount = random.randint(5, 10) * (10 ** 6)
        
        # Create contract instances
        token1 = web3.eth.contract(address=token1_address, abi=ERC20_ABI)

        # Approve token1 (validator token)
        allowance = token1.functions.allowance(wallet_address, fee_manager_address).call()
        if allowance < amount:
            nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
            max_uint256 = 2**256 - 1
            approve_tx = token1.functions.approve(fee_manager_address, max_uint256).build_transaction({
                'from': wallet_address,
                'nonce': nonce,
                'gas': 100000,
                'gasPrice': web3.eth.gas_price,
                'chainId': CONFIG['CHAIN_ID']
            })
            # Sign transaction
            signed_approve = wallet.sign_transaction(approve_tx)
            raw_tx = signed_approve.rawTransaction if hasattr(signed_approve, 'rawTransaction') else signed_approve.raw_transaction
            
            approve_hash = web3.eth.send_raw_transaction(raw_tx)
            
            # Sync wait
            sync_wait_for_tx_with_retry(web3, approve_hash.hex())
            print(f"  → Approve {token1_name} TX: {short_hash(approve_hash.hex())}")

        # Add liquidity: mintWithValidatorToken(validatorToken, feeToken, amount, recipient)
        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        tx = fee_manager.functions.mintWithValidatorToken(
            token1_address,  # validator token
            token2_address,  # fee token
            amount,
            wallet_address
        ).build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 500000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })
        # Sign transaction
        signed_tx = wallet.sign_transaction(tx)
        raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
        
        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        
        # Sync wait
        sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        
        amount_display = amount / (10 ** 6)
        print(f"  → {amount_display} {token1_name} into {token1_name}/{token2_name} pool")
        print(f"  → TX: {short_hash(tx_hash.hex())}")
        return tx_hash.hex()
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity7_set_fee_token(web3, wallet):
    """Set BetaUSD as fee token (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        fee_manager_address = Web3.to_checksum_address(SYSTEM_CONTRACTS['FEE_MANAGER'])
        fee_manager = web3.eth.contract(address=fee_manager_address, abi=FEE_MANAGER_ABI)
        beta_usd_address = Web3.to_checksum_address(CONFIG['TOKENS']['BetaUSD'])
        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        tx = fee_manager.functions.setUserToken(beta_usd_address).build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 100000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })
        # Sign transaction
        signed_tx = wallet.sign_transaction(tx)
        raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
        
        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        print(f"  → Fee token: BetaUSD")
        print(f"  → TX: {short_hash(tx_hash.hex())}")
        return tx_hash.hex()
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity8_mint_tokens(web3, wallet, token_address):
    """Mint tokens for created TIP-20 (Synchronous)"""
    if not token_address:
        return None

    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        token_address_checksum = Web3.to_checksum_address(token_address)
        token = web3.eth.contract(address=token_address_checksum, abi=TIP20_MINT_ABI)
        ISSUER_ROLE = Web3.keccak(text="ISSUER_ROLE")

        # Check role
        needs_role = False
        try:
            has_role = token.functions.hasRole(ISSUER_ROLE, wallet_address).call()
            needs_role = not has_role
        except Exception:
            needs_role = True

        # Grant role if needed
        if needs_role:
            try:
                nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
                grant_tx = token.functions.grantRole(ISSUER_ROLE, wallet_address).build_transaction({
                    'from': wallet_address,
                    'nonce': nonce,
                    'gas': 150000,
                    'gasPrice': web3.eth.gas_price,
                    'chainId': CONFIG['CHAIN_ID']
                })
                # Sign transaction
                signed_grant = wallet.sign_transaction(grant_tx)
                raw_tx = signed_grant.rawTransaction if hasattr(signed_grant, 'rawTransaction') else signed_grant.raw_transaction
                
                grant_tx_hash = web3.eth.send_raw_transaction(raw_tx)
                sync_wait_for_tx_with_retry(web3, grant_tx_hash.hex())
                print(f"  → Grant Role TX: {short_hash(grant_tx_hash.hex())}")
                time.sleep(2)
            except Exception:
                pass

        # Mint
        mint_amount = int(1000 * (10 ** 6))
        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        tx = token.functions.mint(wallet_address, mint_amount).build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 200000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })
        # Sign transaction
        signed_tx = wallet.sign_transaction(tx)
        raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
        
        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        print(f"  → Mint TX: {short_hash(tx_hash.hex())}")
        return tx_hash.hex()
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity9_burn_tokens(web3, wallet, token_address):
    """Burn some tokens (Synchronous)"""
    if not token_address:
        return None

    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        token_address_checksum = Web3.to_checksum_address(token_address)
        token = web3.eth.contract(address=token_address_checksum, abi=TIP20_MINT_ABI)
        balance = token.functions.balanceOf(wallet_address).call()

        if balance >= int(10 * (10 ** 6)):
            burn_amount = int(10 * (10 ** 6))
            nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
            tx = token.functions.burn(burn_amount).build_transaction({
                'from': wallet_address,
                'nonce': nonce,
                'gas': 150000,
                'gasPrice': web3.eth.gas_price,
                'chainId': CONFIG['CHAIN_ID']
            })
            # Sign transaction
            signed_tx = wallet.sign_transaction(tx)
            raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
            
            tx_hash = web3.eth.send_raw_transaction(raw_tx)
            sync_wait_for_tx_with_retry(web3, tx_hash.hex())
            print(f"  → Burn TX: {short_hash(tx_hash.hex())}")
            return tx_hash.hex()
        else:
            print(f"  → Skipping Burn: Balance {balance} < 10")
            return "skipped: low balance"
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        pass
    return None

def activity10_transfer_with_memo(web3, wallet):
    """Transfer with memo (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        path_usd_address = Web3.to_checksum_address(CONFIG['TOKENS']['PathUSD'])
        path_usd = web3.eth.contract(address=path_usd_address, abi=TIP20_MINT_ABI)
        random_address = Web3.to_checksum_address(Account.create().address)
        amount = int(0.01 * (10 ** 6))
        memo_bytes = 'test-memo'.encode('utf-8')[:32].ljust(32, b'\x00')
        memo = '0x' + memo_bytes.hex()

        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        tx = path_usd.functions.transferWithMemo(random_address, amount, memo).build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 150000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })
        # Sign transaction
        signed_tx = wallet.sign_transaction(tx)
        raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
        
        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        print(f"  → 0.01 PathUSD → {short_hash(random_address)}")
        print(f"  → Memo: test-memo")
        print(f"  → TX: {short_hash(tx_hash.hex())}")
        return tx_hash.hex()
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity11_limit_order(web3, wallet):
    """Place random limit order (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        dex_address = Web3.to_checksum_address(SYSTEM_CONTRACTS['STABLECOIN_DEX'])
        dex = web3.eth.contract(address=dex_address, abi=STABLECOIN_DEX_ABI)
        tokens = ['AlphaUSD', 'BetaUSD', 'ThetaUSD']
        random_token = random.choice(tokens)
        token_address = Web3.to_checksum_address(CONFIG['TOKENS'][random_token])
        is_bid = random.random() < 0.5

        amount = int(10 * (10 ** 6))

        if is_bid:
            path_usd = web3.eth.contract(address=Web3.to_checksum_address(CONFIG['TOKENS']['PathUSD']), abi=ERC20_ABI)
            allowance = path_usd.functions.allowance(wallet_address, dex_address).call()
            if allowance < amount:
                nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
                max_uint256 = 2**256 - 1
                approve_tx = path_usd.functions.approve(dex_address, max_uint256).build_transaction({
                    'from': wallet_address,
                    'nonce': nonce,
                    'gas': 100000,
                    'gasPrice': web3.eth.gas_price,
                    'chainId': CONFIG['CHAIN_ID']
                })
                # Sign transaction
                signed_approve = wallet.sign_transaction(approve_tx)
                raw_tx = signed_approve.rawTransaction if hasattr(signed_approve, 'rawTransaction') else signed_approve.raw_transaction
                
                approve_hash = web3.eth.send_raw_transaction(raw_tx)
                sync_wait_for_tx_with_retry(web3, approve_hash.hex())
                print(f"  → Approve TX: {short_hash(approve_hash.hex())}")
        else:
            token = web3.eth.contract(address=token_address, abi=ERC20_ABI)
            allowance = token.functions.allowance(wallet_address, dex_address).call()
            if allowance < amount:
                nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
                max_uint256 = 2**256 - 1
                approve_tx = token.functions.approve(dex_address, max_uint256).build_transaction({
                    'from': wallet_address,
                    'nonce': nonce,
                    'gas': 100000,
                    'gasPrice': web3.eth.gas_price,
                    'chainId': CONFIG['CHAIN_ID']
                })
                # Sign transaction
                signed_approve = wallet.sign_transaction(approve_tx)
                raw_tx = signed_approve.rawTransaction if hasattr(signed_approve, 'rawTransaction') else signed_approve.raw_transaction
                
                approve_hash = web3.eth.send_raw_transaction(raw_tx)
                sync_wait_for_tx_with_retry(web3, approve_hash.hex())
                print(f"  → Approve TX: {short_hash(approve_hash.hex())}")

        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        tx = dex.functions.place(token_address, amount, is_bid, 0).build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 300000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })
        # Sign transaction
        signed_tx = wallet.sign_transaction(tx)
        raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
        
        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        print(f"  → {'Buy' if is_bid else 'Sell'} {random_token}: 10 tokens")
        print(f"  → TX: {short_hash(tx_hash.hex())}")
        return tx_hash.hex()
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity12_remove_liquidity(web3, wallet):
    """Remove liquidity from a random pool (smart: checks all pools) (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        fee_manager = web3.eth.contract(address=Web3.to_checksum_address(SYSTEM_CONTRACTS['FEE_MANAGER']), abi=FEE_MANAGER_ABI)
        
        # Check all possible token pairs for LP balances
        token_names = list(CONFIG['TOKENS'].keys())
        pools_with_balance = []
        
        # Check all combinations of tokens
        for i in range(len(token_names)):
            for j in range(i + 1, len(token_names)):
                token1_name = token_names[i]
                token2_name = token_names[j]
                token1_addr = Web3.to_checksum_address(CONFIG['TOKENS'][token1_name])
                token2_addr = Web3.to_checksum_address(CONFIG['TOKENS'][token2_name])
                
                try:
                    pool_id = fee_manager.functions.getPoolId(token1_addr, token2_addr).call()
                    lp_balance = fee_manager.functions.liquidityBalances(pool_id, wallet_address).call()
                    
                    if lp_balance > 0:
                        pools_with_balance.append({
                            'token1_name': token1_name,
                            'token2_name': token2_name,
                            'token1_addr': token1_addr,
                            'token2_addr': token2_addr,
                            'lp_balance': lp_balance
                        })
                except Exception:
                    # Pool doesn't exist or error reading, skip
                    continue
        
        # If no pools with balance, skip
        if not pools_with_balance:
            print(f"  → No LP positions found")
            return "skipped: no lp"
        
        # Randomly select a pool with balance
        selected_pool = random.choice(pools_with_balance)
        
        # Withdraw random 20-50% of LP balance (at least 0.1 LP)
        lp_balance = selected_pool['lp_balance']
        min_withdraw = int(0.1 * (10 ** 6))  # Minimum 0.1 LP
        
        if lp_balance < min_withdraw:
            print(f"  → LP balance too low: {lp_balance / (10 ** 6):.2f}")
            return "skipped: low balance"
        
        # Random percentage: 20-50%
        withdraw_percent = random.randint(20, 50) / 100
        withdraw_amount = int(lp_balance * withdraw_percent)
        
        # Ensure at least minimum
        withdraw_amount = max(withdraw_amount, min_withdraw)
        # Don't exceed balance
        withdraw_amount = min(withdraw_amount, lp_balance)
        
        # Execute withdrawal
        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        tx = fee_manager.functions.burn(
            selected_pool['token1_addr'],
            selected_pool['token2_addr'],
            withdraw_amount,
            wallet_address
        ).build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 500000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })
        
        # Sign transaction
        signed_tx = wallet.sign_transaction(tx)
        raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
        
        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        
        withdraw_display = withdraw_amount / (10 ** 6)
        balance_display = lp_balance / (10 ** 6)
        print(f"  → Withdrawn: {withdraw_display:.2f} LP ({int(withdraw_percent * 100)}% of {balance_display:.2f})")
        print(f"  → Pool: {selected_pool['token1_name']}/{selected_pool['token2_name']}")
        print(f"  → TX: {short_hash(tx_hash.hex())}")
        return tx_hash.hex()
        
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity13_grant_role(web3, wallet, token_address):
    """Grant PAUSE_ROLE to self (Synchronous)"""
    if not token_address:
        return None

    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        token = web3.eth.contract(address=Web3.to_checksum_address(token_address), abi=TIP20_MINT_ABI)
        PAUSE_ROLE = Web3.keccak(text="PAUSE_ROLE")

        has_role = token.functions.hasRole(PAUSE_ROLE, wallet_address).call()
        if not has_role:
            nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
            grant_tx = token.functions.grantRole(PAUSE_ROLE, wallet_address).build_transaction({
                'from': wallet_address,
                'nonce': nonce,
                'gas': 150000,
                'gasPrice': web3.eth.gas_price,
                'chainId': CONFIG['CHAIN_ID']
            })
            # Sign transaction
            signed_grant = wallet.sign_transaction(grant_tx)
            raw_tx = signed_grant.rawTransaction if hasattr(signed_grant, 'rawTransaction') else signed_grant.raw_transaction
            
            tx_hash = web3.eth.send_raw_transaction(raw_tx)
            sync_wait_for_tx_with_retry(web3, tx_hash.hex())
            print(f"  → Grant PAUSE_ROLE TX: {short_hash(tx_hash.hex())}")
            return tx_hash.hex()
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        pass
    return None

def activity14_nft(web3, wallet):
    """Deploy a simple NFT contract with unique BIP39 name (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        
        # Load BIP39 words for unique names
        try:
            with open('mnemonic.txt', 'r') as f:
                words = [w.strip() for w in f.readlines() if w.strip()]
        except FileNotFoundError:
            words = ['alpha', 'beta', 'gamma', 'delta', 'echo', 'foxtrot']

        # Generate unique 2-word name from BIP39
        word1 = random.choice(words).title()
        word2 = random.choice(words).title()
        nft_name = f"{word1}{word2}"  # e.g., "AbandonAbility"
        
        random_msg = random.choice([
            "Tempo NFT Collection",
            "Digital Art Piece",
            "Blockchain Collectible",
            "Crypto Asset",
            "Web3 Creation"
        ])
        
        # Minimal but unique contract
        simple_contract_source = f'''pragma solidity ^0.8.20;
contract {nft_name}NFT {{
    string public name = "{word1} {word2}";
    string public message = "{random_msg}";
    uint256 public deployedAt = block.timestamp;
    address public creator;
    
    constructor() {{
        creator = msg.sender;
    }}
}}'''
        
        try:
            # Compile quickly
            set_solc_version('0.8.20')
            compiled = compile_source(simple_contract_source, solc_version='0.8.20', optimize=True, optimize_runs=200)
            contract_interface = compiled[f'<stdin>:{nft_name}NFT']
            bytecode = '0x' + contract_interface['bin']
        except Exception:
            # If compilation fails, skip
            print(f"  → Skipping NFT (compilation unavailable)")
            return None

        # Deploy
        contract = web3.eth.contract(abi=contract_interface['abi'], bytecode=bytecode)
        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        transaction = contract.constructor().build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 500000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })

        # Sign transaction
        signed_txn = wallet.sign_transaction(transaction)
        raw_tx = signed_txn.rawTransaction if hasattr(signed_txn, 'rawTransaction') else signed_txn.raw_transaction
        
        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        
        # Sync wait
        receipt = sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        addr = Web3.to_checksum_address(receipt['contractAddress'])
        
        print(f"  → NFT: {word1} {word2}")
        print(f"  → Contract: {addr}")
        print(f"  → TX: {short_hash(tx_hash.hex())}")
        return tx_hash.hex()
        
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity16_retriever_nft(web3, wallet):
    """Retriever NFT (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        nft_contract = web3.eth.contract(address=Web3.to_checksum_address(RETRIEVER_NFT_CONTRACT), abi=RETRIEVER_NFT_ABI)

        allowlist_proof = {
            'proof': [],
            'quantityLimitPerWallet': 2**256 - 1,
            'pricePerToken': 0,
            'currency': '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE'
        }

        nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
        tx = nft_contract.functions.claim(
            wallet_address,
            1,
            '0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE',
            0,
            allowlist_proof,
            b''
        ).build_transaction({
            'from': wallet_address,
            'nonce': nonce,
            'gas': 300000,
            'gasPrice': web3.eth.gas_price,
            'chainId': CONFIG['CHAIN_ID']
        })

        # Sign transaction
        signed_tx = wallet.sign_transaction(tx)
        raw_tx = signed_tx.rawTransaction if hasattr(signed_tx, 'rawTransaction') else signed_tx.raw_transaction
        
        tx_hash = web3.eth.send_raw_transaction(raw_tx)
        sync_wait_for_tx_with_retry(web3, tx_hash.hex())
        print(f"  → Retriever NFT claimed")
        print(f"  → TX: {short_hash(tx_hash.hex())}")
        return tx_hash.hex()
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

def activity17_batch_operations(web3, wallet):
    """Batch Operations (Synchronous)"""
    try:
        wallet_address = Web3.to_checksum_address(wallet.address)
        dex_address = Web3.to_checksum_address(SYSTEM_CONTRACTS['STABLECOIN_DEX'])
        dex = web3.eth.contract(address=dex_address, abi=STABLECOIN_DEX_ABI)
        path_usd_address = Web3.to_checksum_address(CONFIG['TOKENS']['PathUSD'])
        path_usd = web3.eth.contract(address=path_usd_address, abi=ERC20_ABI)
        amount = int(0.5 * (10 ** 6))

        # Approve
        allowance = path_usd.functions.allowance(wallet_address, dex_address).call()
        if allowance < amount:
            nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
            max_uint256 = 2**256 - 1
            approve_tx = path_usd.functions.approve(dex_address, max_uint256).build_transaction({
                'from': wallet_address,
                'nonce': nonce,
                'gas': 100000,
                'gasPrice': web3.eth.gas_price,
                'chainId': CONFIG['CHAIN_ID']
            })
            # Sign transaction
            signed_approve = wallet.sign_transaction(approve_tx)
            raw_tx = signed_approve.rawTransaction if hasattr(signed_approve, 'rawTransaction') else signed_approve.raw_transaction
            
            approve_hash = web3.eth.send_raw_transaction(raw_tx)
            sync_wait_for_tx_with_retry(web3, approve_hash.hex())
            print(f"  → Approve TX: {short_hash(approve_hash.hex())}")

        # Swap
        beta_usd_address = Web3.to_checksum_address(CONFIG['TOKENS']['BetaUSD'])
        quote = dex.functions.quoteSwapExactAmountIn(path_usd_address, beta_usd_address, amount).call()

        if quote > 0:
            min_out = (quote * 99) // 100
            nonce = web3.eth.get_transaction_count(wallet_address, 'pending')
            tx = dex.functions.swapExactAmountIn(path_usd_address, beta_usd_address, amount, min_out).build_transaction({
                'from': wallet_address,
                'nonce': nonce,
                'gas': 300000,
                'gasPrice': web3.eth.gas_price,
                'chainId': CONFIG['CHAIN_ID']
            })
            # Sign transaction
            signed_swap = wallet.sign_transaction(tx)
            raw_tx = signed_swap.rawTransaction if hasattr(signed_swap, 'rawTransaction') else signed_swap.raw_transaction
            
            tx_hash = web3.eth.send_raw_transaction(raw_tx)
            sync_wait_for_tx_with_retry(web3, tx_hash.hex())
            print(f"  → 0.5 PathUSD → BetaUSD")
            print(f"  → Swap TX: {short_hash(tx_hash.hex())}")
            return tx_hash.hex()
        else:
            print(f"  → No liquidity for swap")
            return None
    except Exception as e:
        print(f"  → Error: {str(e)[:60]}")
        return None

from utils.proxy import create_web3_with_proxy, get_proxy_for_key_index

async def run_auto_mode():
    """Main entry for automatic activity mode"""
    print('\n╔═══════════════════════════════════════════════════════════════╗')
    print('║           [21] 🚀 AUTO MODE - Automatic mode                 ║')
    print('╚═══════════════════════════════════════════════════════════════╝\n')

    private_keys = get_private_keys()
    if len(private_keys) == 0:
        print('❌ Private keys not found in pv.txt')
        return

    print(f"\033[1m\033[36mTotal wallets: {len(private_keys)}\033[0m\n")

    # Wallet selection
    print('\033[1m\033[33mSelect wallets for auto mode:\033[0m')
    print('\033[1m\033[32mExamples:\033[0m')
    print('  \033[1m\033[34m5\033[0m - only 5th wallet')
    print('  \033[1m\033[34m1,2,4,8,55\033[0m - specific wallets separated by comma')
    print('  \033[1m\033[34m1-10\033[0m - range from 1 to 10')
    print('  \033[1m\033[34m1-10,68-73\033[0m - multiple ranges')
    print('  \033[1m\033[34m1,5,10-15,20\033[0m - mix: single + ranges')
    print('  \033[1m\033[34mall\033[0m - all wallets\n')

    selection = ask_question('\033[1m\033[36mYour choice: \033[0m')

    if selection.lower().strip() == 'all':
        selected_indices = list(range(len(private_keys)))
        print(f"\n\033[1m\033[32m✓ All wallets selected ({len(private_keys)})\033[0m\n")
    else:
        selected_indices = parse_wallet_selection(selection, len(private_keys))

        if len(selected_indices) == 0:
            print('\n\033[1m\033[31m❌ No wallets selected!\033[0m')
            return

        print(f"\n\033[1m\033[32m✓ Wallets selected: {len(selected_indices)}\033[0m")
        print('\033[1m\033[36mIndexes:\033[0m', ', '.join(str(i + 1) for i in selected_indices))
        print('')

    test_wallets = [private_keys[i] for i in selected_indices]

    for i in range(len(test_wallets)):
        wallet = Account.from_key(test_wallets[i])
        wallet_number = selected_indices[i] + 1
        
        # Setup Web3 with proxy
        proxy = get_proxy_for_key_index(selected_indices[i])
        web3 = create_web3_with_proxy(CONFIG['RPC_URL'], proxy)
        proxy_msg = f"Using Proxy: {proxy}" if proxy else "Using: Direct Connection"
        
        print(f"\n{'=' * 67}")
        print(f"WALLET #{wallet_number} ({i + 1}/{len(test_wallets)}): {wallet.address}")
        print(f"{proxy_msg}")
        print('=' * 67)

        private_key = test_wallets[i]

        # List of all activities
        activities = [
            {'id': 1, 'name': 'Deploy contract', 'fn': lambda: activity1_deploy(web3, wallet)},
            {'id': 2, 'name': 'Faucet', 'fn': lambda: activity2_faucet(web3, wallet)},
            {'id': 3, 'name': 'Send tokens', 'fn': lambda: activity3_send_tokens(web3, wallet)},
            {'id': 4, 'name': 'Create stablecoin', 'fn': lambda: activity4_create_stablecoin(web3, wallet)},
            {'id': 5, 'name': 'Swap', 'fn': lambda: activity5_swap(web3, wallet)},
            {'id': 6, 'name': 'Add liquidity', 'fn': lambda: activity6_add_liquidity(web3, wallet)},
            {'id': 7, 'name': 'Set fee token', 'fn': lambda: activity7_set_fee_token(web3, wallet)},
            {'id': 10, 'name': 'Transfer with memo', 'fn': lambda: activity10_transfer_with_memo(web3, wallet)},
            {'id': 11, 'name': 'Limit order', 'fn': lambda: activity11_limit_order(web3, wallet)},
            {'id': 12, 'name': 'Remove liquidity', 'fn': lambda: activity12_remove_liquidity(web3, wallet)},
            {'id': 14, 'name': 'NFT', 'fn': lambda: activity14_nft(web3, wallet)},
            {'id': 16, 'name': 'Retriever NFT', 'fn': lambda: activity16_retriever_nft(web3, wallet)},
            {'id': 17, 'name': 'Batch Operations', 'fn': lambda: activity17_batch_operations(web3, wallet)}
        ]

        # Shuffle activities per wallet
        shuffled = shuffle_array(activities)

        print('\nExecution order:')
        for idx, act in enumerate(shuffled):
            print(f"  {idx + 1}. [{act['id']}] {act['name']}")
        print('')

        created_token = None

        # Execute activities
        for j in range(len(shuffled)):
            activity = shuffled[j]

            try:
                print(f"\n[{j + 1}/{len(shuffled)}] {activity['name']}...")

                # Special handling for token creation
                if activity['id'] == 4:
                    created_token = await activity['fn']()
                    if created_token:
                        print(f"\033[1m\033[32m  ✓ Token created: {created_token}\033[0m")
                    else:
                        print(f"\033[1m\033[31m  ✗ Token not created\033[0m")

                        # Then immediately mint and burn
                        time.sleep(3)
                        print(f"\n[Bonus] Mint tokens...")
                        mint_result = activity8_mint_tokens(web3, wallet, created_token)
                        if mint_result:
                            print(f"  ✓ Mint done")
                        else:
                            print(f"  ✗ Mint failed")

                        time.sleep(2)
                        print(f"\n[Bonus] Burn tokens...")
                        burn_result = activity9_burn_tokens(web3, wallet, created_token)
                        if burn_result:
                            print(f"  ✓ Burn done")
                        else:
                            print(f"  ✗ Burn failed")

                        time.sleep(2)
                        print(f"\n[Bonus] Grant role...")
                        role_result = activity13_grant_role(web3, wallet, created_token)
                        if role_result:
                            print(f"  ✓ Role granted")
                        else:
                            print(f"  ✗ Role not granted")
                elif activity['id'] in [8, 9, 13]:
                    print(f"  ⊘ Skipped (handled after token creation)")
                elif activity['id'] in [15, 18]:
                    print(f"  ⊘ Skipped (requires specific params)")
                else:
                    result = activity['fn']()
                    if result:
                        print(f"\033[1m\033[32m  ✓ Done\033[0m")
                    else:
                        print(f"\033[1m\033[31m  ✗ Failed\033[0m")

                # Delay between activities
                delay = random.randint(2000, 5000) / 1000
                time.sleep(delay)

            except Exception as error:
                err_msg = str(error)
                if '502' in err_msg or '503' in err_msg:
                    print(f"  ⚠️ RPC error, continuing...")
                else:
                    print(f"  ✗ Error: {err_msg[:60]}")

        print(f"\n✅ Wallet #{wallet_number} finished!")

        # Delay between wallets
        if i < len(test_wallets) - 1:
            delay = random.randint(5000, 10000) / 1000
            print(f"\nWaiting {int(delay)}s before next wallet...")
            await async_sleep(delay)

    print('\n╔═══════════════════════════════════════════════════════════════╗')
    print('║              AUTO MODE FINISHED                               ║')
    print('╚═══════════════════════════════════════════════════════════════╝')
