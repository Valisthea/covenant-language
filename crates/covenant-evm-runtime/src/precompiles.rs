//! Mocked precompile implementations for the Styx primitive suite.
//!
//! These are *test doubles* — they are deterministic, handle-based
//! implementations that let the EVM interpreter exercise Covenant bytecode
//! end-to-end without linking against real TFHE, Dilithium, or ZK libraries.
//!
//! Design:
//!
//! - **FHE (0x101..0x10F)** — handle-based. Every ciphertext is an opaque
//!   32-byte word (`keccak256(b"covenant-fhe" || nonce)`) that lives in
//!   `MockPrecompileState::fhe_handles` alongside its plaintext U256 payload.
//!   Homomorphic ops materialize new handles whose payload is computed
//!   directly on plaintexts — so `reveal_decrypt(handle)` always returns the
//!   real value, and the tests stay deterministic.
//!
//! - **PQ (0x150..0x154)** — signature verification returns `true` by default,
//!   `false` if `state.pq_force_fail` is set. `rand_pq` returns a keccak-based
//!   deterministic stream seeded by `state.pq_nonce`.
//!
//! - **ZK (0x130..0x133)** — proof verification (`ZK_VERIFY` and
//!   `ZK_VDF_VERIFY`) returns `true` by default, `false` if
//!   `state.zk_force_fail` is set; `nullifier` returns `keccak256(secret)`;
//!   `ZK_VDF_EVAL` returns a pass-through value.
//!
//! - **Amnesia (0x120..0x123)** — session counters; tests generally don't
//!   exercise these, but stubs are in place.
//!
//! Every precompile takes `(input: &[u8]) -> Vec<u8>`. The ABI used here is
//! the one assumed by the EVM backend's codegen — 32-byte-aligned big-endian
//! words for scalar inputs, raw byte slices for hashing.

use sha3::{Digest, Keccak256};

use crate::u256::U256;

/// Shared precompile scratchpad. One instance per `MockChain`.
#[derive(Debug, Default)]
pub struct MockPrecompileState {
    /// Next handle to mint.
    fhe_nonce: u64,
    /// Map handle-hash → plaintext payload.
    pub fhe_handles: std::collections::HashMap<[u8; 32], U256>,

    /// Force PQ verify to fail.
    pub pq_force_fail: bool,
    /// Counter for `rand_pq`.
    pub pq_nonce: u64,

    /// Force ZK verify to fail.
    pub zk_force_fail: bool,

    /// Amnesia session id counter.
    pub amnesia_nonce: u64,
}

impl MockPrecompileState {
    /// Mint a fresh FHE handle for `plaintext`.
    pub fn mint_fhe(&mut self, plaintext: U256) -> [u8; 32] {
        self.fhe_nonce += 1;
        let mut hasher = Keccak256::new();
        hasher.update(b"covenant-fhe");
        hasher.update(self.fhe_nonce.to_be_bytes());
        let handle: [u8; 32] = hasher.finalize().into();
        self.fhe_handles.insert(handle, plaintext);
        handle
    }

    /// Resolve a handle to its plaintext payload; zero if unknown (trivial-
    /// encrypt-of-zero is common and we tolerate missing entries gracefully).
    pub fn resolve_fhe(&self, handle: &[u8; 32]) -> U256 {
        self.fhe_handles.get(handle).copied().unwrap_or(U256::ZERO)
    }
}

/// Addresses must match `EvmConfig::default().precompile_addresses`.
pub mod addr {
    // FHE (ERC-8227)
    pub const FHE_ENCRYPT_TRIVIAL: u16 = 0x101;
    pub const FHE_ENCRYPT_FRESH: u16 = 0x102;
    pub const FHE_ADD: u16 = 0x103;
    pub const FHE_SUB: u16 = 0x104;
    pub const FHE_MUL: u16 = 0x105;
    pub const FHE_CMP_EQ: u16 = 0x106;
    pub const FHE_CMP_LT: u16 = 0x107;
    pub const FHE_CMP_GT: u16 = 0x108;
    /// Input: two 32-byte handles. Output: 32-byte bool handle. `a != b`.
    pub const FHE_CMP_NE: u16 = 0x113;
    /// Input: two 32-byte handles. Output: 32-byte bool handle. `a <= b`.
    pub const FHE_CMP_LE: u16 = 0x114;
    pub const FHE_AND: u16 = 0x109;
    pub const FHE_OR: u16 = 0x10A;
    pub const FHE_NOT: u16 = 0x10B;
    pub const FHE_SELECT: u16 = 0x10C;
    pub const FHE_CIPHERTEXT_HASH: u16 = 0x10D;
    pub const FHE_REVEAL_DECRYPT: u16 = 0x10E;
    pub const FHE_BOOTSTRAP: u16 = 0x10F;
    /// Input: 32-byte ciphertext<bool> handle. Output: 1 byte (0 false, 1 true).
    /// Used by AssertEncrypted lowering; triggers revert on the calling contract if 0.
    pub const FHE_THRESHOLD_DECRYPT_BOOL: u16 = 0x110;
    /// Input: 32-byte ciphertext<uint256> handle. Output: 32-byte plaintext.
    /// Used by reveal-winner / finalize paths.
    pub const FHE_THRESHOLD_DECRYPT_UINT256: u16 = 0x111;
    /// Input: two 32-byte handles. Output: 32-byte bool handle. `a >= b`.
    pub const FHE_CMP_GE: u16 = 0x112;
    // Amnesia (ERC-8228)
    pub const AMN_BEGIN: u16 = 0x120;
    pub const AMN_SUBMIT_SHARE: u16 = 0x121;
    pub const AMN_FINALIZE: u16 = 0x122;
    pub const AMN_DESTRUCTION_PROOF: u16 = 0x123;
    // ZK (ERC-8229)
    pub const ZK_VERIFY: u16 = 0x130;
    pub const ZK_NULLIFIER: u16 = 0x131;
    pub const ZK_VDF_EVAL: u16 = 0x132;
    pub const ZK_VDF_VERIFY: u16 = 0x133;
    // PQ (ERC-8231)
    pub const PQ_VERIFY_DILITHIUM: u16 = 0x150;
    pub const PQ_HYBRID_VERIFY: u16 = 0x151;
    pub const PQ_RAND: u16 = 0x152;
    pub const PQ_KYBER_ENCRYPT: u16 = 0x153;
    pub const PQ_KYBER_DECRYPT: u16 = 0x154;
}

/// Convenience: the set of addresses considered precompiles by the mock.
pub fn is_precompile(addr_low: u16) -> bool {
    matches!(
        addr_low,
        addr::FHE_ENCRYPT_TRIVIAL..=addr::FHE_CMP_LE
            | addr::AMN_BEGIN..=addr::AMN_DESTRUCTION_PROOF
            | addr::ZK_VERIFY..=addr::ZK_VDF_VERIFY
            | addr::PQ_VERIFY_DILITHIUM..=addr::PQ_KYBER_DECRYPT
    )
}

/// Dispatch a precompile call. Returns the output bytes. Missing / malformed
/// input is tolerated with zero-padding where reasonable.
///
/// KSR-CVN-020: calldata is prefixed with a 4-byte opcode selector. The mock
/// strips it before operand parsing. Callers constructing inputs by hand may
/// omit it; the helper tolerates payloads shorter than 4 bytes by treating
/// them as operand bytes directly.
pub fn dispatch(addr_low: u16, input: &[u8], state: &mut MockPrecompileState) -> Vec<u8> {
    let input: &[u8] = if input.len() >= 4 { &input[4..] } else { input };
    use addr::*;
    match addr_low {
        // ---- FHE ----
        FHE_ENCRYPT_TRIVIAL => {
            // Input: 32 bytes plaintext. Output: 32-byte handle.
            let pt = read_word(input, 0);
            let handle = state.mint_fhe(pt);
            handle.to_vec()
        }
        FHE_ENCRYPT_FRESH => {
            // Same behavior as trivial for testing purposes.
            let pt = read_word(input, 0);
            let handle = state.mint_fhe(pt);
            handle.to_vec()
        }
        FHE_ADD | FHE_SUB | FHE_MUL | FHE_AND | FHE_OR => {
            let a = state.resolve_fhe(&read_word_bytes(input, 0));
            let b = state.resolve_fhe(&read_word_bytes(input, 32));
            let out = match addr_low {
                FHE_ADD => a.wrapping_add(&b),
                FHE_SUB => a.wrapping_sub(&b),
                FHE_MUL => a.wrapping_mul(&b),
                FHE_AND => a.bitand(&b),
                FHE_OR => a.bitor(&b),
                _ => unreachable!(),
            };
            let handle = state.mint_fhe(out);
            handle.to_vec()
        }
        FHE_NOT => {
            let a = state.resolve_fhe(&read_word_bytes(input, 0));
            let handle = state.mint_fhe(a.bitnot());
            handle.to_vec()
        }
        FHE_CMP_EQ | FHE_CMP_NE | FHE_CMP_LT | FHE_CMP_LE | FHE_CMP_GT | FHE_CMP_GE => {
            let a = state.resolve_fhe(&read_word_bytes(input, 0));
            let b = state.resolve_fhe(&read_word_bytes(input, 32));
            let result: U256 = match addr_low {
                FHE_CMP_EQ => (a == b).into(),
                FHE_CMP_NE => (a != b).into(),
                FHE_CMP_LT => (a < b).into(),
                FHE_CMP_LE => (a <= b).into(),
                FHE_CMP_GT => (a > b).into(),
                FHE_CMP_GE => (a >= b).into(),
                _ => unreachable!(),
            };
            let handle = state.mint_fhe(result);
            handle.to_vec()
        }
        FHE_SELECT => {
            // select(cond_handle, then_handle, else_handle)
            let c = state.resolve_fhe(&read_word_bytes(input, 0));
            let t = state.resolve_fhe(&read_word_bytes(input, 32));
            let e = state.resolve_fhe(&read_word_bytes(input, 64));
            let result = if !c.is_zero() { t } else { e };
            let handle = state.mint_fhe(result);
            handle.to_vec()
        }
        FHE_CIPHERTEXT_HASH => {
            // Hash the handle itself for determinism.
            let h = read_word_bytes(input, 0);
            let mut hasher = Keccak256::new();
            hasher.update(h);
            hasher.finalize().to_vec()
        }
        FHE_REVEAL_DECRYPT => {
            let h = read_word_bytes(input, 0);
            let pt = state.resolve_fhe(&h);
            pt.to_be_bytes().to_vec()
        }
        FHE_BOOTSTRAP => {
            // Pass-through: bootstrap is identity on plaintext payloads.
            read_word_bytes(input, 0).to_vec()
        }
        FHE_THRESHOLD_DECRYPT_BOOL => {
            // Input: 32-byte handle for ciphertext<bool>.
            // Output: single byte — 1 if plaintext is non-zero, else 0.
            // V0.2 note: leaks 1 bit about the ciphertext, acceptable for
            // balance-sufficient checks (caller already knows the result by
            // initiating the transfer). See LESSONS.md §7.
            let h = read_word_bytes(input, 0);
            let pt = state.resolve_fhe(&h);
            vec![if !pt.is_zero() { 1u8 } else { 0u8 }]
        }
        FHE_THRESHOLD_DECRYPT_UINT256 => {
            // Input: 32-byte handle for ciphertext<uint256>.
            // Output: 32-byte plaintext.
            let h = read_word_bytes(input, 0);
            let pt = state.resolve_fhe(&h);
            pt.to_be_bytes().to_vec()
        }

        // ---- PQ ----
        PQ_VERIFY_DILITHIUM | PQ_HYBRID_VERIFY => {
            let ok = !state.pq_force_fail;
            bool_word(ok)
        }
        PQ_RAND => {
            state.pq_nonce += 1;
            let mut hasher = Keccak256::new();
            hasher.update(b"covenant-pq-rand");
            hasher.update(state.pq_nonce.to_be_bytes());
            hasher.finalize().to_vec()
        }
        PQ_KYBER_ENCRYPT => {
            // Return the input hashed: determinstic stand-in for a KEM ciphertext.
            let mut hasher = Keccak256::new();
            hasher.update(input);
            hasher.finalize().to_vec()
        }
        PQ_KYBER_DECRYPT => {
            // Pass-through.
            input.to_vec()
        }

        // ---- ZK ----
        ZK_VERIFY => bool_word(!state.zk_force_fail),
        ZK_NULLIFIER => {
            let mut hasher = Keccak256::new();
            hasher.update(input);
            hasher.finalize().to_vec()
        }
        ZK_VDF_EVAL => {
            // Identity pass-through on first 32 bytes.
            read_word_bytes(input, 0).to_vec()
        }
        // OMEGA V6 (MED-005 fix): this used to be hardcoded `true` with no
        // fail-mode at all, unlike every other verify-shaped precompile
        // here (PQ_VERIFY_DILITHIUM/PQ_HYBRID_VERIFY gate on pq_force_fail,
        // ZK_VERIFY gates on zk_force_fail) -- a test could never exercise
        // "VDF proof rejected" against the mock, even though the real
        // Sepolia-deployed helper can fail. Reuses zk_force_fail (VDF
        // verification is the same ZK-proof-verification family as
        // ZK_VERIFY) rather than adding a third flag.
        ZK_VDF_VERIFY => bool_word(!state.zk_force_fail),

        // ---- Amnesia ----
        AMN_BEGIN => {
            state.amnesia_nonce += 1;
            U256::from_u64(state.amnesia_nonce).to_be_bytes().to_vec()
        }
        AMN_SUBMIT_SHARE | AMN_FINALIZE | AMN_DESTRUCTION_PROOF => bool_word(true),

        _ => Vec::new(),
    }
}

fn read_word(input: &[u8], off: usize) -> U256 {
    U256::from_be_bytes(read_word_bytes(input, off))
}

fn read_word_bytes(input: &[u8], off: usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    let end = (off + 32).min(input.len());
    if off < end {
        out[..end - off].copy_from_slice(&input[off..end]);
    }
    out
}

fn bool_word(b: bool) -> Vec<u8> {
    let mut out = vec![0u8; 32];
    if b {
        out[31] = 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prepend a 4-byte dummy selector (KSR-CVN-020 wire format). The mock
    /// dispatch strips it unconditionally.
    fn with_sel(bytes: &[u8]) -> Vec<u8> {
        let mut v = vec![0u8; 4];
        v.extend_from_slice(bytes);
        v
    }

    #[test]
    fn fhe_encrypt_then_decrypt() {
        let mut st = MockPrecompileState::default();
        let mut pt = [0u8; 32];
        pt[31] = 42;
        let handle_bytes = dispatch(addr::FHE_ENCRYPT_TRIVIAL, &with_sel(&pt), &mut st);
        let decrypted = dispatch(addr::FHE_REVEAL_DECRYPT, &with_sel(&handle_bytes), &mut st);
        assert_eq!(&decrypted[..], &pt[..]);
    }

    #[test]
    fn fhe_add() {
        let mut st = MockPrecompileState::default();
        let a = U256::from_u64(10).to_be_bytes();
        let b = U256::from_u64(32).to_be_bytes();
        let ha = dispatch(addr::FHE_ENCRYPT_TRIVIAL, &with_sel(&a), &mut st);
        let hb = dispatch(addr::FHE_ENCRYPT_TRIVIAL, &with_sel(&b), &mut st);
        let mut inputs = Vec::new();
        inputs.extend(&ha);
        inputs.extend(&hb);
        let hsum = dispatch(addr::FHE_ADD, &with_sel(&inputs), &mut st);
        let result = dispatch(addr::FHE_REVEAL_DECRYPT, &with_sel(&hsum), &mut st);
        assert_eq!(
            U256::from_be_bytes(result.try_into().unwrap()).low_u64(),
            42
        );
    }

    #[test]
    fn pq_verify_default_ok() {
        let mut st = MockPrecompileState::default();
        let out = dispatch(addr::PQ_VERIFY_DILITHIUM, &with_sel(&[]), &mut st);
        assert_eq!(out[31], 1);
        st.pq_force_fail = true;
        let out = dispatch(addr::PQ_VERIFY_DILITHIUM, &with_sel(&[]), &mut st);
        assert_eq!(out[31], 0);
    }

    #[test]
    fn zk_vdf_verify_respects_force_fail_ksr_cvn_med_005() {
        // OMEGA V6 MED-005 regression test: ZK_VDF_VERIFY used to be
        // hardcoded `true` with no fail-mode at all, unlike every other
        // verify-shaped precompile here -- a test could never exercise
        // "VDF proof rejected" against the mock.
        let mut st = MockPrecompileState::default();
        let out = dispatch(addr::ZK_VDF_VERIFY, &with_sel(&[]), &mut st);
        assert_eq!(out[31], 1, "default must remain pass (no behavior change)");
        st.zk_force_fail = true;
        let out = dispatch(addr::ZK_VDF_VERIFY, &with_sel(&[]), &mut st);
        assert_eq!(out[31], 0, "zk_force_fail must now gate VDF verify too");
    }

    #[test]
    fn fhe_threshold_decrypt_bool_true() {
        let mut st = MockPrecompileState::default();
        let handle = st.mint_fhe(U256::from_u64(1));
        let out = dispatch(
            addr::FHE_THRESHOLD_DECRYPT_BOOL,
            &with_sel(&handle),
            &mut st,
        );
        assert_eq!(
            out,
            vec![1u8],
            "non-zero ciphertext must threshold-decrypt to 1"
        );
    }

    #[test]
    fn fhe_threshold_decrypt_bool_false() {
        let mut st = MockPrecompileState::default();
        let handle = st.mint_fhe(U256::ZERO);
        let out = dispatch(
            addr::FHE_THRESHOLD_DECRYPT_BOOL,
            &with_sel(&handle),
            &mut st,
        );
        assert_eq!(
            out,
            vec![0u8],
            "zero ciphertext must threshold-decrypt to 0"
        );
    }

    #[test]
    fn fhe_threshold_decrypt_uint256_returns_plaintext() {
        let mut st = MockPrecompileState::default();
        let value = U256::from_u64(12345);
        let handle = st.mint_fhe(value);
        let out = dispatch(
            addr::FHE_THRESHOLD_DECRYPT_UINT256,
            &with_sel(&handle),
            &mut st,
        );
        assert_eq!(out.len(), 32, "threshold_decrypt_uint256 returns 32 bytes");
        assert_eq!(U256::from_be_bytes(out.try_into().unwrap()), value);
    }

    #[test]
    fn fhe_unknown_handle_resolves_to_zero() {
        let st = MockPrecompileState::default();
        let unknown = [0u8; 32]; // not minted
        let result = st.resolve_fhe(&unknown);
        assert_eq!(result, U256::ZERO);
    }
}
