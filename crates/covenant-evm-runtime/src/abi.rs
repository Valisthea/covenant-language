//! Minimal ABI helpers for assembling calldata and decoding return values.
//!
//! The Covenant EVM backend uses a simplified ABI: every scalar argument is
//! one 32-byte word, regardless of underlying type. That convention lets us
//! keep encoding/decoding very small here.

use sha3::{Digest, Keccak256};

use crate::address::Address;
use crate::u256::U256;

/// Compute the full 32-byte event topic (keccak256 of the event signature).
/// Unlike selectors, event topics use all 32 bytes.
pub fn event_topic(sig: &str) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(sig.as_bytes());
    hasher.finalize().into()
}

/// Decode ABI-encoded dynamic `string` return data.
/// Expected layout: [offset (32B)][length (32B)][data (left-aligned, padded to 32B)].
pub fn decode_string(data: &[u8]) -> String {
    if data.len() < 64 {
        return String::new();
    }
    // Offset is the first 32-byte word (big-endian). Take the low 4 bytes.
    let offset = u32::from_be_bytes([data[28], data[29], data[30], data[31]]) as usize;
    if data.len() < offset + 32 {
        return String::new();
    }
    // Length is at `offset`, low 4 bytes of that 32-byte word.
    let len = u32::from_be_bytes([
        data[offset + 28],
        data[offset + 29],
        data[offset + 30],
        data[offset + 31],
    ]) as usize;
    let data_start = offset + 32;
    if data.len() < data_start + len {
        return String::new();
    }
    String::from_utf8_lossy(&data[data_start..data_start + len]).into_owned()
}

/// Compute the 4-byte selector for a function signature like "transfer(address,uint256)".
pub fn selector(sig: &str) -> [u8; 4] {
    let mut hasher = Keccak256::new();
    hasher.update(sig.as_bytes());
    let h: [u8; 32] = hasher.finalize().into();
    [h[0], h[1], h[2], h[3]]
}

/// Build calldata: selector + concatenated 32-byte words.
pub fn encode_call(sig: &str, args: &[U256]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + args.len() * 32);
    out.extend_from_slice(&selector(sig));
    for a in args {
        out.extend_from_slice(&a.to_be_bytes());
    }
    out
}

/// Build calldata using raw 32-byte words (convenient for opaque handles).
pub fn encode_call_words(sig: &str, words: &[[u8; 32]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + words.len() * 32);
    out.extend_from_slice(&selector(sig));
    for w in words {
        out.extend_from_slice(w);
    }
    out
}

/// Decode return data as a single U256 (assumes 32-byte response).
pub fn decode_u256(data: &[u8]) -> U256 {
    let mut buf = [0u8; 32];
    let end = data.len().min(32);
    buf[..end].copy_from_slice(&data[..end]);
    U256::from_be_bytes(buf)
}

/// Decode return data as an Address (assumes 32-byte response, last 20 bytes).
pub fn decode_address(data: &[u8]) -> Address {
    Address::from_u256(decode_u256(data))
}

/// Decode return data as a bool (assumes 32-byte response).
pub fn decode_bool(data: &[u8]) -> bool {
    !decode_u256(data).is_zero()
}

/// Build a 32-byte word from an Address (right-aligned).
pub fn addr_word(a: Address) -> U256 {
    a.to_u256()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_selectors() {
        // ERC-20 transfer
        assert_eq!(
            selector("transfer(address,uint256)"),
            [0xa9, 0x05, 0x9c, 0xbb]
        );
        assert_eq!(selector("balanceOf(address)"), [0x70, 0xa0, 0x82, 0x31]);
        assert_eq!(selector("totalSupply()"), [0x18, 0x16, 0x0d, 0xdd]);
    }

    #[test]
    fn encode_transfer() {
        let data = encode_call(
            "transfer(address,uint256)",
            &[Address::from_low_u64(0x2001).to_u256(), U256::from_u64(100)],
        );
        assert_eq!(&data[..4], &[0xa9, 0x05, 0x9c, 0xbb]);
        assert_eq!(data.len(), 4 + 64);
    }
}
