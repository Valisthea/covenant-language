//! 20-byte Ethereum address.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::u256::U256;

/// 20-byte Ethereum-style address.
///
/// `Serialize` is implemented manually so the JSON form is the
/// `"0x"`-prefixed lowercase hex string everything in the playground
/// (and every block explorer) expects, instead of the default
/// derive-driven `[200, 17, …]` byte array.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Address(pub [u8; 20]);

impl Address {
    pub const ZERO: Address = Address([0u8; 20]);

    /// Build an address from a trailing-low u64 (high bytes zero).
    pub fn from_low_u64(v: u64) -> Address {
        let mut bytes = [0u8; 20];
        bytes[12..20].copy_from_slice(&v.to_be_bytes());
        Address(bytes)
    }

    /// As u256 word (right-aligned).
    pub fn to_u256(self) -> U256 {
        let mut buf = [0u8; 32];
        buf[12..32].copy_from_slice(&self.0);
        U256::from_be_bytes(buf)
    }

    /// Low 20 bytes of a u256 as address.
    pub fn from_u256(v: U256) -> Address {
        let b = v.to_be_bytes();
        let mut out = [0u8; 20];
        out.copy_from_slice(&b[12..32]);
        Address(out)
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    /// Parse a `"0x"`-prefixed (or bare) 40-char hex string into an address.
    ///
    /// Tolerant of mixed case and the `0x` prefix being absent. Returns
    /// `Err` with a human-readable message on length / hex-digit failures
    /// so the playground can surface the bad input to the user.
    pub fn from_hex(s: &str) -> Result<Address, String> {
        let trimmed = s
            .strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .unwrap_or(s);
        if trimmed.len() != 40 {
            return Err(format!(
                "address must be 40 hex chars (got {})",
                trimmed.len()
            ));
        }
        let bytes = hex::decode(trimmed).map_err(|e| format!("invalid hex: {e}"))?;
        let mut out = [0u8; 20];
        out.copy_from_slice(&bytes);
        Ok(Address(out))
    }

    /// Borrow the underlying 20 bytes. Mirrors the `[u8; 20]` field
    /// access (`addr.0`) but reads more naturally at call sites that
    /// take a slice.
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

impl Serialize for Address {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("0x{}", self.to_hex()))
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Address::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl From<[u8; 20]> for Address {
    fn from(b: [u8; 20]) -> Self {
        Address(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_u64_and_u256_roundtrip() {
        let a = Address::from_low_u64(0x1234);
        let w = a.to_u256();
        let b = Address::from_u256(w);
        assert_eq!(a, b);
    }

    #[test]
    fn from_hex_round_trips() {
        let a = Address::from_low_u64(0xdeadbeef);
        let s = format!("0x{}", a.to_hex());
        let parsed = Address::from_hex(&s).unwrap();
        assert_eq!(a, parsed);
    }

    #[test]
    fn from_hex_tolerates_no_prefix_and_mixed_case() {
        let a = Address::from_hex("DeAdBeEf00000000000000000000000000000001").unwrap();
        let b = Address::from_hex("0xdeadbeef00000000000000000000000000000001").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn from_hex_rejects_wrong_length() {
        assert!(Address::from_hex("0x1234").is_err());
        assert!(Address::from_hex("not-hex").is_err());
    }

    #[test]
    fn json_round_trip_is_lowercase_0x_hex() {
        let a = Address::from_low_u64(0xabc);
        let json = serde_json::to_string(&a).unwrap();
        // Quoted "0x..." string, lowercase, 40 hex chars.
        assert_eq!(json, "\"0x0000000000000000000000000000000000000abc\"");
        let parsed: Address = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, a);
    }
}
