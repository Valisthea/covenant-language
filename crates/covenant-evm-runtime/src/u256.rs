//! Minimal 256-bit unsigned integer (big-endian byte storage).
//!
//! This is *not* a fully-featured U256. It covers exactly what the Covenant
//! EVM mock interpreter and its tests need: bytewise serialization, equality,
//! bitwise and arithmetic ops, comparisons, shifts, and a few helpful
//! constructors. It is not constant-time and makes no claim to be; tests do
//! not need constant-time math.
//!
//! Internal representation: `limbs: [u64; 4]`, little-endian (limbs[0] is
//! least significant). Byte serialization is big-endian as per EVM convention.

// Direct-index loops over the fixed four limbs are clearer than enumerate
// here, and the arithmetic kernels stay symmetric across limb positions.
#![allow(clippy::needless_range_loop)]

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A 256-bit unsigned integer.
///
/// `Serialize`/`Deserialize` are hand-written so the JSON shape is the
/// `"0x"`-prefixed minimal-length hex string everything in EVM tooling
/// uses for u256 values (balances, slot keys, return data lengths).
/// Deriving would expose the internal little-endian limb array, which
/// the playground would then have to re-pack: friction we don't need.
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct U256 {
    /// Little-endian limbs (limbs[0] is LSB).
    pub limbs: [u64; 4],
}

impl U256 {
    pub const ZERO: U256 = U256 { limbs: [0; 4] };
    pub const ONE: U256 = U256 {
        limbs: [1, 0, 0, 0],
    };
    pub const MAX: U256 = U256 {
        limbs: [u64::MAX; 4],
    };

    /// From big-endian 32 bytes.
    pub fn from_be_bytes(bytes: [u8; 32]) -> Self {
        let mut limbs = [0u64; 4];
        for i in 0..4 {
            let start = (3 - i) * 8;
            limbs[i] = u64::from_be_bytes(bytes[start..start + 8].try_into().unwrap());
        }
        U256 { limbs }
    }

    /// To big-endian 32 bytes.
    pub fn to_be_bytes(self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for i in 0..4 {
            let start = (3 - i) * 8;
            out[start..start + 8].copy_from_slice(&self.limbs[i].to_be_bytes());
        }
        out
    }

    pub fn from_u64(v: u64) -> Self {
        U256 {
            limbs: [v, 0, 0, 0],
        }
    }

    pub fn from_u128(v: u128) -> Self {
        U256 {
            limbs: [v as u64, (v >> 64) as u64, 0, 0],
        }
    }

    /// Return the low 64 bits.
    pub fn low_u64(&self) -> u64 {
        self.limbs[0]
    }

    /// Return the low 128 bits.
    pub fn low_u128(&self) -> u128 {
        (self.limbs[0] as u128) | ((self.limbs[1] as u128) << 64)
    }

    /// True iff value is zero.
    pub fn is_zero(&self) -> bool {
        self.limbs.iter().all(|&l| l == 0)
    }

    /// True iff value fits in 64 bits.
    pub fn fits_u64(&self) -> bool {
        self.limbs[1] == 0 && self.limbs[2] == 0 && self.limbs[3] == 0
    }

    /// True iff value fits in usize (host).
    pub fn fits_usize(&self) -> bool {
        self.fits_u64() && self.limbs[0] <= usize::MAX as u64
    }

    /// Try to return as usize, clamped to `u32::MAX` on overflow. Intended for
    /// memory offsets: callers typically further bound the result.
    pub fn as_usize_saturating(&self) -> usize {
        if !self.fits_u64() {
            return usize::MAX;
        }
        let v = self.limbs[0];
        if v > (u32::MAX as u64) {
            usize::MAX
        } else {
            v as usize
        }
    }

    /// Wrapping addition.
    pub fn wrapping_add(&self, other: &U256) -> U256 {
        let mut out = [0u64; 4];
        let mut carry: u128 = 0;
        for i in 0..4 {
            let sum = (self.limbs[i] as u128) + (other.limbs[i] as u128) + carry;
            out[i] = sum as u64;
            carry = sum >> 64;
        }
        U256 { limbs: out }
    }

    /// Wrapping subtraction.
    pub fn wrapping_sub(&self, other: &U256) -> U256 {
        // Two's complement: a - b = a + (!b + 1)
        let neg = other.bitnot().wrapping_add(&U256::ONE);
        self.wrapping_add(&neg)
    }

    /// Wrapping multiplication.
    pub fn wrapping_mul(&self, other: &U256) -> U256 {
        let mut out = [0u64; 4];
        for i in 0..4 {
            let mut carry: u128 = 0;
            for j in 0..(4 - i) {
                let prod = (self.limbs[i] as u128) * (other.limbs[j] as u128)
                    + (out[i + j] as u128)
                    + carry;
                out[i + j] = prod as u64;
                carry = prod >> 64;
            }
        }
        U256 { limbs: out }
    }

    /// Integer division. Returns zero if divisor is zero (EVM semantics).
    pub fn div(&self, other: &U256) -> U256 {
        if other.is_zero() {
            return U256::ZERO;
        }
        let (q, _) = divmod(*self, *other);
        q
    }

    /// Integer modulo. Returns zero if divisor is zero (EVM semantics).
    pub fn rem(&self, other: &U256) -> U256 {
        if other.is_zero() {
            return U256::ZERO;
        }
        let (_, r) = divmod(*self, *other);
        r
    }

    /// Bitwise AND.
    pub fn bitand(&self, other: &U256) -> U256 {
        let mut out = [0u64; 4];
        for i in 0..4 {
            out[i] = self.limbs[i] & other.limbs[i];
        }
        U256 { limbs: out }
    }

    /// Bitwise OR.
    pub fn bitor(&self, other: &U256) -> U256 {
        let mut out = [0u64; 4];
        for i in 0..4 {
            out[i] = self.limbs[i] | other.limbs[i];
        }
        U256 { limbs: out }
    }

    /// Bitwise XOR.
    pub fn bitxor(&self, other: &U256) -> U256 {
        let mut out = [0u64; 4];
        for i in 0..4 {
            out[i] = self.limbs[i] ^ other.limbs[i];
        }
        U256 { limbs: out }
    }

    /// Bitwise NOT.
    pub fn bitnot(&self) -> U256 {
        let mut out = [0u64; 4];
        for i in 0..4 {
            out[i] = !self.limbs[i];
        }
        U256 { limbs: out }
    }

    /// Logical shift left by `shift` bits (clamped to 256).
    pub fn shl(&self, shift: u32) -> U256 {
        if shift >= 256 {
            return U256::ZERO;
        }
        let word_shift = (shift / 64) as usize;
        let bit_shift = shift % 64;
        let mut out = [0u64; 4];
        for i in 0usize..4 {
            let src = match i.checked_sub(word_shift) {
                Some(s) if s < 4 => self.limbs[s],
                _ => 0,
            };
            out[i] = if bit_shift == 0 {
                src
            } else {
                let lower_src = match i.checked_sub(word_shift).and_then(|s| s.checked_sub(1)) {
                    Some(s) if s < 4 => self.limbs[s],
                    _ => 0,
                };
                (src << bit_shift) | (lower_src >> (64 - bit_shift))
            };
        }
        U256 { limbs: out }
    }

    /// Logical shift right by `shift` bits (clamped to 256).
    pub fn shr(&self, shift: u32) -> U256 {
        if shift >= 256 {
            return U256::ZERO;
        }
        let word_shift = (shift / 64) as usize;
        let bit_shift = shift % 64;
        let mut out = [0u64; 4];
        for i in 0usize..4 {
            let src = if i + word_shift < 4 {
                self.limbs[i + word_shift]
            } else {
                0
            };
            out[i] = if bit_shift == 0 {
                src
            } else {
                let upper_src = if i + word_shift + 1 < 4 {
                    self.limbs[i + word_shift + 1]
                } else {
                    0
                };
                (src >> bit_shift) | (upper_src << (64 - bit_shift))
            };
        }
        U256 { limbs: out }
    }

    /// Big-endian hex string, lowercased, without `0x` prefix.
    pub fn to_hex(&self) -> String {
        hex::encode(self.to_be_bytes())
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> Ordering {
        for i in (0..4).rev() {
            match self.limbs[i].cmp(&other.limbs[i]) {
                Ordering::Equal => continue,
                o => return o,
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Debug for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U256(0x{})", self.to_hex())
    }
}

impl fmt::Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl Serialize for U256 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Drop leading zero bytes so the hex is minimal-length, like
        // every JSON-RPC chain client expects. ZERO becomes `"0x0"`.
        let bytes = self.to_be_bytes();
        let first_nonzero = bytes.iter().position(|b| *b != 0).unwrap_or(31);
        let trimmed = &bytes[first_nonzero..];
        let hex_str = hex::encode(trimmed);
        // Strip a single leading zero if the first nibble is 0, keeps
        // single-digit values as `"0x1"` instead of `"0x01"`.
        let stripped = hex_str.strip_prefix('0').unwrap_or(&hex_str);
        let display = if stripped.is_empty() { "0" } else { stripped };
        s.serialize_str(&format!("0x{display}"))
    }
}

impl<'de> Deserialize<'de> for U256 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let trimmed = s
            .strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .unwrap_or(&s);
        if trimmed.is_empty() {
            return Ok(U256::ZERO);
        }
        // Left-pad with a single zero if odd-length so hex::decode is happy.
        let padded;
        let to_decode: &str = if trimmed.len() % 2 == 1 {
            padded = format!("0{trimmed}");
            &padded
        } else {
            trimmed
        };
        let raw = hex::decode(to_decode).map_err(serde::de::Error::custom)?;
        if raw.len() > 32 {
            return Err(serde::de::Error::custom("u256 overflow (> 32 bytes)"));
        }
        let mut padded32 = [0u8; 32];
        padded32[32 - raw.len()..].copy_from_slice(&raw);
        Ok(U256::from_be_bytes(padded32))
    }
}

impl From<u64> for U256 {
    fn from(v: u64) -> Self {
        U256::from_u64(v)
    }
}

impl From<u32> for U256 {
    fn from(v: u32) -> Self {
        U256::from_u64(v as u64)
    }
}

impl From<bool> for U256 {
    fn from(v: bool) -> Self {
        if v {
            U256::ONE
        } else {
            U256::ZERO
        }
    }
}

impl From<[u8; 32]> for U256 {
    fn from(b: [u8; 32]) -> Self {
        U256::from_be_bytes(b)
    }
}

/// Binary long division (Knuth algorithm D, simplified). Sufficient for tests.
fn divmod(numer: U256, denom: U256) -> (U256, U256) {
    debug_assert!(!denom.is_zero());
    if numer < denom {
        return (U256::ZERO, numer);
    }
    // Bit-by-bit long division.
    let mut quotient = U256::ZERO;
    let mut remainder = U256::ZERO;
    for i in (0..256).rev() {
        // remainder <<= 1
        remainder = remainder.shl(1);
        // remainder |= bit i of numer
        let limb_idx = i / 64;
        let bit_idx = i % 64;
        let bit = (numer.limbs[limb_idx] >> bit_idx) & 1;
        remainder.limbs[0] |= bit;
        if remainder >= denom {
            remainder = remainder.wrapping_sub(&denom);
            quotient.limbs[limb_idx] |= 1u64 << bit_idx;
        }
    }
    (quotient, remainder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_be_roundtrip() {
        let mut bytes = [0u8; 32];
        bytes[31] = 42;
        let v = U256::from_be_bytes(bytes);
        assert_eq!(v.low_u64(), 42);
        assert_eq!(v.to_be_bytes(), bytes);
    }

    #[test]
    fn arithmetic_basic() {
        let a = U256::from_u64(100);
        let b = U256::from_u64(7);
        assert_eq!(a.wrapping_add(&b), U256::from_u64(107));
        assert_eq!(a.wrapping_sub(&b), U256::from_u64(93));
        assert_eq!(a.wrapping_mul(&b), U256::from_u64(700));
        assert_eq!(a.div(&b), U256::from_u64(14));
        assert_eq!(a.rem(&b), U256::from_u64(2));
    }

    #[test]
    fn bitwise() {
        let a = U256::from_u64(0b1100);
        let b = U256::from_u64(0b1010);
        assert_eq!(a.bitand(&b), U256::from_u64(0b1000));
        assert_eq!(a.bitor(&b), U256::from_u64(0b1110));
        assert_eq!(a.bitxor(&b), U256::from_u64(0b0110));
    }

    #[test]
    fn shifts() {
        let a = U256::from_u64(1);
        assert_eq!(a.shl(8), U256::from_u64(0x100));
        assert_eq!(a.shl(64).limbs[1], 1);
        assert_eq!(a.shl(256), U256::ZERO);
        let b = U256::from_u64(0xFF00);
        assert_eq!(b.shr(8), U256::from_u64(0xFF));
    }

    #[test]
    fn ordering() {
        let a = U256::from_u64(100);
        let b = U256::from_u64(200);
        assert!(a < b);
        assert!(b > a);
        assert_eq!(a, a);
    }

    #[test]
    fn div_by_zero_is_zero() {
        assert_eq!(U256::from_u64(100).div(&U256::ZERO), U256::ZERO);
        assert_eq!(U256::from_u64(100).rem(&U256::ZERO), U256::ZERO);
    }

    #[test]
    fn wrapping_add_overflow() {
        assert_eq!(U256::MAX.wrapping_add(&U256::ONE), U256::ZERO);
    }
}
