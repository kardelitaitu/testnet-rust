# ═══════════════════════════════════════════════════════════════════════════════
# SMART AUTOMATION MODULE
# ═══════════════════════════════════════════════════════════════════════════════

import asyncio
import random
import string
from web3 import Web3
from eth_account import Account
from config import (
    CONFIG, ERC20_ABI, COLORS, SYSTEM_CONTRACTS, 
    INFINITY_NAME_CONTRACT, FEE_MANAGER_ABI,
    TIP403_REGISTRY, TIP403_REGISTRY_ABI, TIP20_POLICY_ABI
)
from utils.proxy import create_web3_with_proxy, get_proxy_for_key_index
from utils.helpers import async_sleep, short_hash, wait_for_tx_with_retry, sync_wait_for_tx_with_retry
from utils.wallet import load_created_tokens

# Import activities from auto module
from modules.auto import (
    activity1_deploy,
    activity2_faucet,
    activity3_send_tokens,
    activity4_create_stablecoin,
    activity5_swap,
    activity6_add_liquidity,
    activity7_set_fee_token,
    activity8_mint_tokens,
    activity9_burn_tokens,
    activity10_transfer_with_memo,
    activity11_limit_order,
    activity12_remove_liquidity,
    activity13_grant_role,
    activity14_nft,
    activity16_retriever_nft,
    activity17_batch_operations
)

# Local implementation of missing activities (to avoid input prompts)
INFINITY_NAME_ABI = [
    {'constant': False, 'inputs': [{'name': 'domain', 'type': 'string'}, {'name': 'referrer', 'type': 'address'}], 'name': 'register', 'outputs': [{'name': '', 'type': 'uint256'}], 'type': 'function'},
    {'constant': True, 'inputs': [{'name': 'domain', 'type': 'string'}], 'name': 'isAvailable', 'outputs': [{'name': '', 'type': 'bool'}], 'type': 'function'}
]

class SmartWallet:
    def __init__(self, index, private_key, use_proxy=True):
        self.index = index
        self.private_key = private_key
        self.wallet = Account.from_key(private_key)
        self.address = self.wallet.address
        
        # Option to disable proxies
        if use_proxy:
            self.proxy = get_proxy_for_key_index(index)
        else:
            self.proxy = None
        
        # Select random RPC for load balancing
        rpc_list = CONFIG.get('RPC_LIST', [CONFIG['RPC_URL']])
        self.rpc_selected = random.choice(rpc_list)
        
        self.web3 = create_web3_with_proxy(self.rpc_selected, self.proxy)
        self.balances = {}
        self.history = {
            'created_tokens': [],
            'last_action': None
        }
        self.should_recover_faucet = False

    def update_balances(self):
        """Fetch current ETH and token balances (Sync for threading)"""
        try:
            # Native ETH (fee token)
            eth_balance = self.web3.eth.get_balance(self.address)
            self.balances['ETH'] = eth_balance / 10**18

            # Main tokens
            for symbol in ['PathUSD', 'AlphaUSD', 'BetaUSD', 'ThetaUSD']:
                token_addr = CONFIG['TOKENS'].get(symbol)
                if token_addr:
                    contract = self.web3.eth.contract(address=Web3.to_checksum_address(token_addr), abi=ERC20_ABI)
                    bal = contract.functions.balanceOf(self.address).call()
                    self.balances[symbol] = bal / 10**6  # Assuming 6 decimals
        except Exception as e:
            error_str = str(e)
            # Only print warning if it's not a connection/timeout error
            if 'connection' not in error_str.lower() and 'timeout' not in error_str.lower():
                print(f"  ⚠️ Balance check failed: {str(e)[:50]}")
            # Set default balances on error
            self.balances['ETH'] = 0

    def decide_next_action(self):
        """Decide the best next action based on state (Sync for threading)"""
        # If last action failed due to gas, try to force faucet
        if self.should_recover_faucet:
            self.should_recover_faucet = False
            return {
                'name': 'Faucet Claim (Recovery)',
                'fn': lambda: activity2_faucet(self.web3, self.wallet),
                'reason': "Recovering from Insufficient Funds"
            }

        self.update_balances()
        
        eth = self.balances.get('ETH', 0)
        path = self.balances.get('PathUSD', 0)
        alpha = self.balances.get('AlphaUSD', 0)
        beta = self.balances.get('BetaUSD', 0)

        # Priority 1: Faucet if low ETH
        if eth < 2.0:
            return {
                'name': 'Faucet Claim (Low Balance)',
                'fn': lambda: activity2_faucet(self.web3, self.wallet),
                'reason': f"Low ETH: {eth:.4f} < 2.0"
            }

        # Priority 2: Create Token if we haven't created one this session or rarely
        if random.random() < 0.15: 
            return {
                'name': 'Deploy New Token',
                'fn': self._wrap_create_token,
                'reason': "Building portfolio"
            }

        # Priority 3: Swap if we have PathUSD but low Alpha/Beta
        if path > 10 and (alpha < 5 or beta < 5):
            return {
                'name': 'Swap PathUSD -> Stable',
                'fn': lambda: activity5_swap(self.web3, self.wallet),
                'reason': f"Balancing portfolio (Path: {path:.1f})"
            }

        # Priority 4: Infinity Name (Rare)
        if path > 1000 and random.random() < 0.1:
            return {
                'name': 'Infinity Name Register',
                'fn': lambda: self.activity15_infinity_name(),
                'reason': "Domain registration"
            }

        # Priority 5: Add Liquidity if we have both Path + Alpha
        if path > 5 and alpha > 5 and random.random() < 0.3:
            return {
                'name': 'Add Liquidity',
                'fn': lambda: activity6_add_liquidity(self.web3, self.wallet),
                'reason': "Capital efficiency"
            }

        # Priority 6: TIP-403 Policy (If we have tokens)
        my_tokens = load_created_tokens().get(Web3.to_checksum_address(self.address), [])
        if my_tokens and random.random() < 0.1:
            return {
                'name': 'TIP-403 Policy',
                'fn': lambda: self.activity18_tip403_policies(my_tokens),
                'reason': "Security policy"
            }

        # Priority 7: Analytics (Interactive simulation)
        if random.random() < 0.05:
            return {
                'name': 'Analytics & Stats',
                'fn': lambda: self.activity_analytics(),
                'reason': "Monitoring"
            }

        # Priority 8: Mint/Burn on our own tokens if we have them
        if self.history['created_tokens'] and random.random() < 0.4:
            token = random.choice(self.history['created_tokens'])
            is_mint = random.random() < 0.6
            if is_mint:
                 return {
                    'name': f'Mint Own Token',
                    'fn': lambda: activity8_mint_tokens(self.web3, self.wallet, token),
                    'reason': "Managing own asset"
                }
            else:
                 return {
                    'name': f'Burn Own Token',
                    'fn': lambda: activity9_burn_tokens(self.web3, self.wallet, token),
                    'reason': "Burning supply"
                }

        # Fallback: Random safe activity
        options = [
            {'name': 'Deploy Contract', 'fn': lambda: activity1_deploy(self.web3, self.wallet)},
            {'name': 'NFT Mint', 'fn': lambda: activity14_nft(self.web3, self.wallet)},
            {'name': 'Send Dust', 'fn': lambda: activity3_send_tokens(self.web3, self.wallet)},
            {'name': 'Limit Order', 'fn': lambda: activity11_limit_order(self.web3, self.wallet)},
            {'name': 'Retriever NFT', 'fn': lambda: activity16_retriever_nft(self.web3, self.wallet)},
            {'name': 'Batch Operations', 'fn': lambda: activity17_batch_operations(self.web3, self.wallet)},
        ]
        
        choice = random.choice(options)
        choice['reason'] = "Random activity"
        return choice

    def _wrap_create_token(self):
        """Wrapper to capture created token address (Synchronous)"""
        token = activity4_create_stablecoin(self.web3, self.wallet)
        if token:
            self.history['created_tokens'].append(token)
            from utils.wallet import save_created_token
            save_created_token(self.address, token, 'TUSD-GEN')
        return token

    # ═══════════════════════════════════════════════════════════════════════════════
    # LOCAL IMPLEMENTATIONS
    # ═══════════════════════════════════════════════════════════════════════════════

    def activity15_infinity_name(self):
        """Auto-register random Infinity Name (Synchronous)"""
        try:
            domain = ''.join(random.choice(string.ascii_lowercase + string.digits) for _ in range(10))
            path_usd = self.web3.eth.contract(address=Web3.to_checksum_address(CONFIG['TOKENS']['PathUSD']), abi=ERC20_ABI)
            infinity = self.web3.eth.contract(address=Web3.to_checksum_address(INFINITY_NAME_CONTRACT), abi=INFINITY_NAME_ABI)
            
            # Approve
            amount = 1000 * 10**6
            allowance = path_usd.functions.allowance(self.address, infinity.address).call()
            if allowance < amount:
                nonce = self.web3.eth.get_transaction_count(self.address, 'pending')
                tx = path_usd.functions.approve(infinity.address, 2**256 - 1).build_transaction({
                    'from': self.address, 'nonce': nonce, 'gas': 100000, 
                    'gasPrice': self.web3.eth.gas_price, 'chainId': CONFIG['CHAIN_ID']
                })
                # Sign & Send (reuse wait_for_tx_with_retry logic pattern)
                signed = self.wallet.sign_transaction(tx)
                raw_tx = getattr(signed, 'rawTransaction', None) or getattr(signed, 'raw_transaction', None)
                tx_hash = self.web3.eth.send_raw_transaction(raw_tx)
                sync_wait_for_tx_with_retry(self.web3, tx_hash.hex())

            # Register
            print(f"  → Registering {domain}.tempo")
            nonce = self.web3.eth.get_transaction_count(self.address, 'pending')
            tx = infinity.functions.register(domain, '0x0000000000000000000000000000000000000000').build_transaction({
                'from': self.address, 'nonce': nonce, 'gas': 500000, 
                'gasPrice': self.web3.eth.gas_price, 'chainId': CONFIG['CHAIN_ID']
            })
            signed = self.wallet.sign_transaction(tx)
            raw_tx = getattr(signed, 'rawTransaction', None) or getattr(signed, 'raw_transaction', None)
            tx_hash = self.web3.eth.send_raw_transaction(raw_tx)
            sync_wait_for_tx_with_retry(self.web3, tx_hash.hex())
            print(f"  → Registered {domain}.tempo")
            return True
        except Exception as e:
            print(f"  → Infinity Error: {str(e)[:50]}")
            return False

    def activity18_tip403_policies(self, my_tokens):
        """Auto-create TIP-403 Policy for own token (Synchronous)"""
        try:
            token_info = random.choice(my_tokens)
            token_addr = Web3.to_checksum_address(token_info['token'])
            
            # Create random whitelist
            random_whitelist = [Web3.to_checksum_address(Account.create().address) for _ in range(3)]
            random_whitelist.append(self.address) # Add self
            
            registry = self.web3.eth.contract(address=Web3.to_checksum_address(TIP403_REGISTRY), abi=TIP403_REGISTRY_ABI)
            token = self.web3.eth.contract(address=token_addr, abi=TIP20_POLICY_ABI)
            
            print(f"  → Creating Policy for {token_info['symbol']}")
            
            # Create Policy
            nonce = self.web3.eth.get_transaction_count(self.address, 'pending')
            tx = registry.functions.createPolicyWithAccounts(self.address, 0, random_whitelist).build_transaction({
                'from': self.address, 'nonce': nonce, 'gas': 500000, 
                'gasPrice': self.web3.eth.gas_price, 'chainId': CONFIG['CHAIN_ID']
            })
            signed = self.wallet.sign_transaction(tx)
            raw_tx = getattr(signed, 'rawTransaction', None) or getattr(signed, 'raw_transaction', None)
            tx_hash = self.web3.eth.send_raw_transaction(raw_tx)
            receipt = sync_wait_for_tx_with_retry(self.web3, tx_hash.hex())
            
            # Extract Policy ID (Simplified)
            policy_id = 0
            if receipt['logs']:
                 policy_id = int.from_bytes(receipt['logs'][0]['topics'][1][-8:], 'big')
            
            if policy_id == 0:
                print(f"  → Failed to get Policy ID")
                return False

            # Attach
            nonce = self.web3.eth.get_transaction_count(self.address, 'pending')
            tx = token.functions.changeTransferPolicyId(policy_id).build_transaction({
                'from': self.address, 'nonce': nonce, 'gas': 200000, 
                'gasPrice': self.web3.eth.gas_price, 'chainId': CONFIG['CHAIN_ID']
            })
            signed = self.wallet.sign_transaction(tx)
            raw_tx = getattr(signed, 'rawTransaction', None) or getattr(signed, 'raw_transaction', None)
            tx_hash = self.web3.eth.send_raw_transaction(raw_tx)
            # sync_wait_for_tx_with_retry returns receipt, but we don't need it here
            sync_wait_for_tx_with_retry(self.web3, tx_hash.hex())
            print(f"  → Policy {policy_id} attached")
            return True
        except Exception as e:
            print(f"  → TIP403 Error: {str(e)[:50]}")
            return False
        return False # Fallback safety

    def activity_analytics(self):
        """Simulate checking analytics (Synchronous)"""
        # Calculate total net worth from known stablecoins
        total_value = sum(self.balances.get(sym, 0) for sym in ['PathUSD', 'AlphaUSD', 'BetaUSD', 'ThetaUSD'])
        
        print(f"  → Analyzed Wallet: {self.address}")
        print(f"  → Net Worth: {total_value:.2f} USD")
        print(f"  → Tokens: {len(self.balances)} assets detected")
        return True

    def print_status(self):
        proxy_msg = f"Proxy: {self.proxy}" if self.proxy else "Direct"
        eth = self.balances.get('ETH', 0)
        path = self.balances.get('PathUSD', 0)
        
        print(f"\033[1m\033[36m[{self.index+1}] {self.address[:6]}...{self.address[-4:]} | ETH: {eth:.4f} | PathUSD: {path:.1f} | {proxy_msg}\033[0m")
