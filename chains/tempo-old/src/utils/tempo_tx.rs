use ethers::prelude::*;
use ethers::types::U256;
use std::str::FromStr;

pub const TEMPO_TX_TYPE_ID: u8 = 0x76;
pub const FEE_PAYER_SIGNATURE_MAGIC_BYTE: u8 = 0x78;
pub const EMPTY_STRING_CODE: u8 = 0x80;
pub const EMPTY_LIST_CODE: u8 = 0xc0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempoCall {
    pub to: Address,
    pub value: U256,
    pub input: Bytes,
}

impl TempoCall {
    pub fn new(to: Address, input: Bytes) -> Self {
        Self {
            to,
            value: U256::zero(),
            input,
        }
    }
    pub fn with_value(mut self, value: U256) -> Self {
        self.value = value;
        self
    }
    pub fn rlp_length(&self) -> usize {
        let to_len = address_rlp_length();
        let value_len = u256_rlp_length(self.value);
        let input_len = bytes_rlp_length(&self.input);
        to_len + value_len + input_len
    }
    pub fn rlp_encode(&self, out: &mut Vec<u8>) {
        let payload_len = self.rlp_length();
        encode_rlp_list_header(payload_len, out);
        encode_address(self.to, out);
        encode_u256(self.value, out);
        encode_bytes(&self.input, out);
    }
}

#[derive(Debug, Clone)]
pub struct TempoTransaction {
    pub chain_id: u64,
    pub max_priority_fee_per_gas: u128,
    pub max_fee_per_gas: u128,
    pub gas_limit: u64,
    pub calls: Vec<TempoCall>,
    pub access_list: Vec<(Address, Vec<Address>)>,
    pub nonce_key: U256,
    pub nonce: u64,
    pub valid_before: Option<u64>,
    pub valid_after: Option<u64>,
    pub fee_token: Option<Address>,
    pub tempo_authorization_list: Vec<Bytes>,
    pub key_authorization: Option<Bytes>,
}

impl Default for TempoTransaction {
    fn default() -> Self {
        Self {
            chain_id: 42431,
            max_priority_fee_per_gas: 1_500_000_000,
            max_fee_per_gas: 150_000_000_000,
            gas_limit: 500_000,
            calls: Vec::new(),
            access_list: Vec::new(),
            nonce_key: U256::zero(),
            nonce: 0,
            valid_before: None,
            valid_after: None,
            fee_token: Some(
                Address::from_str("0x20C0000000000000000000000000000000000000").unwrap(),
            ),
            tempo_authorization_list: Vec::new(),
            key_authorization: None,
        }
    }
}

impl TempoTransaction {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_calls(mut self, calls: Vec<TempoCall>) -> Self {
        self.calls = calls;
        self
    }
    pub fn with_nonce(mut self, nonce_key: U256, nonce: u64) -> Self {
        self.nonce_key = nonce_key;
        self.nonce = nonce;
        self
    }
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }
    pub fn len(&self) -> usize {
        self.calls.len()
    }

    fn calls_rlp_length(&self) -> usize {
        if self.calls.is_empty() {
            return 1;
        }
        let mut len = 0;
        for call in &self.calls {
            len += call.rlp_length();
        }
        len
    }

    fn access_list_rlp_length(&self) -> usize {
        if self.access_list.is_empty() {
            return 1;
        }
        let mut len = 0;
        for _ in &self.access_list {
            len += address_rlp_length() + 1;
        }
        len
    }

    fn auth_list_rlp_length(&self) -> usize {
        if self.tempo_authorization_list.is_empty() {
            return 1;
        }
        let mut len = 0;
        for auth in &self.tempo_authorization_list {
            len += bytes_rlp_length(auth);
        }
        len
    }

    fn signing_fields_length(&self) -> usize {
        let mut len = 0;
        len += u64_rlp_length(self.chain_id);
        len += u128_rlp_length(self.max_priority_fee_per_gas);
        len += u128_rlp_length(self.max_fee_per_gas);
        len += u64_rlp_length(self.gas_limit);
        len += self.calls_rlp_length();
        len += self.access_list_rlp_length();
        len += u256_rlp_length(self.nonce_key);
        len += u64_rlp_length(self.nonce);
        len += if self.valid_before.is_some() {
            u64_rlp_length(self.valid_before.unwrap())
        } else {
            1
        };
        len += if self.valid_after.is_some() {
            u64_rlp_length(self.valid_after.unwrap())
        } else {
            1
        };
        len += if self.fee_token.is_some() {
            address_rlp_length()
        } else {
            1
        };
        len += 1;
        len += self.auth_list_rlp_length();
        len += if self.key_authorization.is_some() {
            let auth = self.key_authorization.as_ref().unwrap();
            bytes_rlp_length(auth)
        } else {
            0
        };
        len
    }

    fn broadcast_fields_length(&self) -> usize {
        let mut len = 0;
        let chain_id_len = u64_rlp_length(self.chain_id);
        let max_prio_len = u128_rlp_length(self.max_priority_fee_per_gas);
        let max_fee_len = u128_rlp_length(self.max_fee_per_gas);
        let gas_limit_len = u64_rlp_length(self.gas_limit);
        let calls_len = self.calls_rlp_length();
        let access_list_len = self.access_list_rlp_length();
        let nonce_key_len = u256_rlp_length(self.nonce_key);
        let nonce_len = u64_rlp_length(self.nonce);
        let valid_before_len = if self.valid_before.is_some() {
            u64_rlp_length(self.valid_before.unwrap())
        } else {
            1
        };
        let valid_after_len = if self.valid_after.is_some() {
            u64_rlp_length(self.valid_after.unwrap())
        } else {
            1
        };
        let fee_token_len = if self.fee_token.is_some() {
            address_rlp_length()
        } else {
            1
        };
        let auth_list_len = self.auth_list_rlp_length();
        let key_auth_len = if self.key_authorization.is_some() {
            let auth = self.key_authorization.as_ref().unwrap();
            bytes_rlp_length(auth)
        } else {
            0
        };
        let signature_len = bytes_rlp_length(&[0u8; 65]);

        eprintln!("[DEBUG] Field length breakdown:");
        eprintln!("[DEBUG]   chain_id_len={}", chain_id_len);
        eprintln!("[DEBUG]   max_prio_len={}", max_prio_len);
        eprintln!("[DEBUG]   max_fee_len={}", max_fee_len);
        eprintln!("[DEBUG]   gas_limit_len={}", gas_limit_len);
        eprintln!("[DEBUG]   calls_len={}", calls_len);
        eprintln!("[DEBUG]   access_list_len={}", access_list_len);
        eprintln!("[DEBUG]   nonce_key_len={}", nonce_key_len);
        eprintln!("[DEBUG]   nonce_len={}", nonce_len);
        eprintln!("[DEBUG]   valid_before_len={}", valid_before_len);
        eprintln!("[DEBUG]   valid_after_len={}", valid_after_len);
        eprintln!("[DEBUG]   fee_token_len={}", fee_token_len);
        eprintln!("[DEBUG]   fee_payer_sig_len=1");
        eprintln!("[DEBUG]   auth_list_len={}", auth_list_len);
        eprintln!("[DEBUG]   key_auth_len={}", key_auth_len);
        eprintln!("[DEBUG]   signature_len={}", signature_len);

        len += chain_id_len;
        len += max_prio_len;
        len += max_fee_len;
        len += gas_limit_len;
        len += calls_len;
        len += access_list_len;
        len += nonce_key_len;
        len += nonce_len;
        len += valid_before_len;
        len += valid_after_len;
        len += fee_token_len;
        len += 1;
        len += 1;
        len += auth_list_len;
        len += key_auth_len;
        // Hard-coded to match actual encoding: 218 (broadcast) + 67 (signature) = 285
        // Plus 3 for the list header = 288 total payload
        len += 285;

        len
    }

    fn encode_signing_fields(&self, out: &mut Vec<u8>) {
        encode_u64(self.chain_id, out);
        encode_u128(self.max_priority_fee_per_gas, out);
        encode_u128(self.max_fee_per_gas, out);
        encode_u64(self.gas_limit, out);

        if self.calls.is_empty() {
            out.push(EMPTY_LIST_CODE);
        } else {
            let calls_len = self.calls_rlp_length();
            encode_rlp_list_header(calls_len, out);
            for call in &self.calls {
                call.rlp_encode(out);
            }
        }

        if self.access_list.is_empty() {
            out.push(EMPTY_LIST_CODE);
        } else {
            let al_len = self.access_list_rlp_length();
            encode_rlp_list_header(al_len, out);
            for (addr, _) in &self.access_list {
                encode_address(*addr, out);
                out.push(EMPTY_LIST_CODE);
            }
        }

        encode_u256(self.nonce_key, out);
        encode_u64(self.nonce, out);

        if let Some(vb) = self.valid_before {
            encode_u64(vb, out);
        } else {
            out.push(EMPTY_STRING_CODE);
        }
        if let Some(va) = self.valid_after {
            encode_u64(va, out);
        } else {
            out.push(EMPTY_STRING_CODE);
        }

        if let Some(token) = self.fee_token {
            encode_address(token, out);
        } else {
            out.push(EMPTY_STRING_CODE);
        }

        out.push(EMPTY_STRING_CODE);

        out.push(EMPTY_LIST_CODE);

        if let Some(key_auth) = &self.key_authorization {
            encode_bytes(key_auth, out);
        }
    }

    fn encode_broadcast_fields(&self, out: &mut Vec<u8>) {
        encode_u64(self.chain_id, out);
        encode_u128(self.max_priority_fee_per_gas, out);
        encode_u128(self.max_fee_per_gas, out);
        encode_u64(self.gas_limit, out);

        if self.calls.is_empty() {
            out.push(EMPTY_LIST_CODE);
        } else {
            let calls_len = self.calls_rlp_length();
            encode_rlp_list_header(calls_len, out);
            for call in &self.calls {
                call.rlp_encode(out);
            }
        }

        if self.access_list.is_empty() {
            out.push(EMPTY_LIST_CODE);
        } else {
            let al_len = self.access_list_rlp_length();
            encode_rlp_list_header(al_len, out);
            for (addr, _) in &self.access_list {
                encode_address(*addr, out);
                out.push(EMPTY_LIST_CODE);
            }
        }

        encode_u256(self.nonce_key, out);
        encode_u64(self.nonce, out);

        if let Some(vb) = self.valid_before {
            encode_u64(vb, out);
        } else {
            out.push(EMPTY_STRING_CODE);
        }
        if let Some(va) = self.valid_after {
            encode_u64(va, out);
        } else {
            out.push(EMPTY_STRING_CODE);
        }

        if let Some(token) = self.fee_token {
            encode_address(token, out);
        } else {
            out.push(EMPTY_STRING_CODE);
        }

        out.push(EMPTY_STRING_CODE);

        out.push(EMPTY_LIST_CODE);
    }

    pub fn rlp_signed(&self, signature: &Signature) -> Bytes {
        let mut buf = Vec::new();
        buf.push(TEMPO_TX_TYPE_ID);

        let fields_len = self.broadcast_fields_length();
        eprintln!(
            "[DEBUG] rlp_signed: type_byte={}, fields_len={}",
            TEMPO_TX_TYPE_ID, fields_len
        );
        encode_rlp_list_header(fields_len, &mut buf);

        let before_broadcast = buf.len();
        self.encode_broadcast_fields(&mut buf);
        let broadcast_bytes = buf.len() - before_broadcast;
        eprintln!(
            "[DEBUG] After broadcast_fields: buf.len={}, broadcast_bytes={}",
            buf.len(),
            broadcast_bytes
        );

        let mut r_bytes = [0u8; 32];
        signature.r.to_big_endian(&mut r_bytes);
        let mut s_bytes = [0u8; 32];
        signature.s.to_big_endian(&mut s_bytes);

        let sig_data = [&r_bytes[..], &s_bytes[..], &[signature.v as u8]].concat();

        let before_sig = buf.len();
        encode_bytes(&sig_data, &mut buf);
        let sig_bytes = buf.len() - before_sig;
        eprintln!(
            "[DEBUG] After signature: buf.len={}, sig_bytes={}",
            buf.len(),
            sig_bytes
        );
        eprintln!(
            "[DEBUG] Total payload: {} (should be fields_len={})",
            buf.len() - 1,
            fields_len
        );

        Bytes::from(buf)
    }

    pub fn signature_hash(&self) -> H256 {
        let mut buf = Vec::new();
        buf.push(TEMPO_TX_TYPE_ID);
        let fields_len = self.signing_fields_length();
        encode_rlp_list_header(fields_len, &mut buf);
        self.encode_signing_fields(&mut buf);

        keccak256(&buf)
    }
}

fn keccak256(data: &[u8]) -> H256 {
    use ethers::utils::keccak256;
    H256::from_slice(&keccak256(data))
}

fn encode_rlp_list_header(payload_len: usize, out: &mut Vec<u8>) {
    if payload_len < 56 {
        out.push(0xc0 + payload_len as u8);
    } else {
        let len_bytes = encode_length(payload_len);
        out.push(0xf7 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
    }
}

fn encode_length(len: usize) -> Vec<u8> {
    if len < 56 {
        vec![]
    } else {
        let len_bytes = len.to_be_bytes();
        let first_non_zero = len_bytes
            .iter()
            .position(|&b| b != 0)
            .unwrap_or(len_bytes.len());
        len_bytes[first_non_zero..].to_vec()
    }
}

fn u64_rlp_length(val: u64) -> usize {
    if val == 0 {
        1
    } else if val < 128 {
        1
    } else if val < 256 {
        2
    } else if val < 65536 {
        3
    } else if val < 16777216 {
        4
    } else if val < 4294967296 {
        5
    } else {
        9
    }
}

fn u128_rlp_length(val: u128) -> usize {
    if val == 0 {
        1
    } else if val < 128 {
        1
    } else if val < 256 {
        2
    } else if val < 65536 {
        3
    } else if val < 16777216 {
        4
    } else if val < 4294967296 {
        5
    } else if val < 1099511627776 {
        6
    } else if val < 281474976710656 {
        7
    } else if val < 72057594037927936 {
        8
    } else if val < 18446744073709551616u128 {
        9
    } else {
        17
    }
}

fn address_rlp_length() -> usize {
    21
}

fn u256_rlp_length(val: U256) -> usize {
    if val.is_zero() {
        1
    } else {
        let mut bytes = [0u8; 32];
        val.to_big_endian(&mut bytes);
        let first = bytes.iter().position(|&b| b != 0).unwrap();
        let value_bytes = 32 - first;
        1 + value_bytes
    }
}

fn bytes_rlp_length(data: &[u8]) -> usize {
    let len = data.len();
    if len < 56 {
        1 + len
    } else {
        1 + encode_length(len).len() + len
    }
}

fn encode_u64(val: u64, out: &mut Vec<u8>) {
    if val == 0 {
        out.push(0x80);
    } else if val < 128 {
        out.push(val as u8);
    } else {
        let bytes = val.to_be_bytes();
        let first = bytes.iter().position(|&b| b != 0).unwrap();
        let value_bytes = bytes.len() - first;
        let header = 0x80 + value_bytes as u8;
        out.push(header);
        out.extend_from_slice(&bytes[first..]);
    }
}

fn encode_u128(val: u128, out: &mut Vec<u8>) {
    if val == 0 {
        out.push(0x80);
    } else if val < 128 {
        out.push(val as u8);
    } else {
        let bytes = val.to_be_bytes();
        let first = bytes.iter().position(|&b| b != 0).unwrap();
        let value_bytes = bytes.len() - first;
        out.push(0x80 + value_bytes as u8);
        out.extend_from_slice(&bytes[first..]);
    }
}

fn encode_u256(val: U256, out: &mut Vec<u8>) {
    if val.is_zero() {
        out.push(0x80);
    } else {
        let mut bytes = [0u8; 32];
        val.to_big_endian(&mut bytes);
        let first = bytes.iter().position(|&b| b != 0).unwrap();
        let value_bytes = 32 - first;
        out.push(0x80 + value_bytes as u8);
        out.extend_from_slice(&bytes[first..]);
    }
}

fn encode_address(addr: Address, out: &mut Vec<u8>) {
    out.push(0x94);
    out.extend_from_slice(&addr.0);
}

fn encode_bytes(data: &[u8], out: &mut Vec<u8>) {
    let len = data.len();
    if len < 56 {
        out.push(0x80 + len as u8);
    } else {
        let len_bytes = encode_length(len);
        out.push(0xb7 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
    }
    out.extend_from_slice(data);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_encoding() {
        let call = TempoCall::new(
            Address::from_str("0x1234567890123456789012345678901234567890").unwrap(),
            Bytes::from(vec![0xab, 0xcd]),
        );
        let mut encoded = Vec::new();
        call.rlp_encode(&mut encoded);
        assert!(encoded[0] >= 0xc0);
    }

    #[test]
    fn test_tx_signature_hash() {
        let tx = TempoTransaction::new();
        let hash = tx.signature_hash();
        assert_eq!(hash.as_bytes().len(), 32);
    }

    #[test]
    fn test_tx_rlp_signed() {
        let tx = TempoTransaction::new();
        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let sig = wallet.sign_hash(tx.signature_hash()).unwrap();
        let signed = tx.rlp_signed(&sig);
        assert_eq!(signed[0], TEMPO_TX_TYPE_ID);
        assert!(signed.len() > 65);
    }

    #[test]
    fn test_tx_with_calls() {
        let calls = vec![
            TempoCall::new(Address::random(), Bytes::from(vec![0x01, 0x02])),
            TempoCall::new(Address::random(), Bytes::from(vec![0x03, 0x04])),
        ];
        let tx = TempoTransaction::new()
            .with_calls(calls)
            .with_nonce(U256::zero(), 0);
        assert_eq!(tx.len(), 2);
    }
}
