//! Stdlib function and module-method signatures.
//!
//! All signatures hardcoded from Doc 3. Variadic functions are represented with
//! `VariadicKind` to describe how trailing arguments are checked.

use covenant_resolver::{StdlibFn, StdlibModule as RM};
// Re-expose under the original name so the public API of this module still uses
// `StdlibModule`. Internal references below use `RM::` to sidestep a glob-import
// conflict with `Ty::StdlibModule` (the type-system variant).
pub use covenant_resolver::StdlibModule;

use crate::ty::{StdlibModuleTag, Ty};

/// How trailing arguments of a variadic function are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum VariadicKind {
    /// Not variadic; arity must match exactly.
    Fixed,
    /// `keccak(...)`, `encode(...)` — accept any hashable/encodable trailing args.
    AnyHashable,
    /// Trailing args must all share the same concrete type as the last declared param.
    /// Reserved for future use; no current stdlib signature uses this mode.
    LastParam,
}

#[derive(Debug, Clone)]
pub struct Signature {
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub variadic: VariadicKind,
}

impl Signature {
    pub const fn fixed(params: Vec<Ty>, ret: Ty) -> Self {
        Self {
            params,
            ret,
            variadic: VariadicKind::Fixed,
        }
    }
}

/// Look up the signature of a stdlib free function.
pub fn stdlib_fn_signature(f: StdlibFn) -> Signature {
    use StdlibFn::*;
    use Ty::*;
    match f {
        Encrypted => Signature {
            params: vec![Unknown],
            ret: Ciphertext(Box::new(Unknown)),
            variadic: VariadicKind::Fixed,
        },
        CiphertextHashOf => Signature {
            params: vec![Ciphertext(Box::new(Unknown))],
            ret: Hash,
            variadic: VariadicKind::Fixed,
        },
        Keccak => Signature {
            params: vec![Bytes],
            ret: Hash,
            variadic: VariadicKind::AnyHashable,
        },
        Encode => Signature {
            params: vec![Unknown],
            ret: Bytes,
            variadic: VariadicKind::AnyHashable,
        },
        Decode => Signature {
            params: vec![Bytes],
            ret: Unknown,
            variadic: VariadicKind::Fixed,
        },
        Destroy => Signature::fixed(vec![Unknown], Unit),
        Freeze => Signature::fixed(vec![Unknown], Unit),
        RandPq => Signature::fixed(vec![Bytes], Hash),
        Min | Max => Signature {
            params: vec![Amount, Amount],
            ret: Amount,
            variadic: VariadicKind::Fixed,
        },
        Abs => Signature::fixed(vec![Amount], Amount),
        Pow => Signature::fixed(vec![Amount, Amount], Amount),
        Sqrt => Signature::fixed(vec![Amount], Amount),
        DecryptInTest => Signature::fixed(vec![Ciphertext(Box::new(Unknown))], Unknown),
        MeasureGas => Signature::fixed(vec![], Unknown),
        ExpectRevert => Signature::fixed(vec![Unknown], Unit),
        AsCaller => Signature::fixed(vec![Address], Unit),
        AdvanceTime => Signature::fixed(vec![Duration], Unit),
        Deploy => Signature::fixed(vec![Text, List(Box::new(Bytes))], Address),
        RandomAddress => Signature::fixed(vec![], Address),
    }
}

/// Look up a method on a stdlib module. Returns `None` if unknown.
pub fn stdlib_method_signature(module: StdlibModule, method: &str) -> Option<Signature> {
    use Ty::*;
    let sig = match (module, method) {
        // -------- PQKeys --------
        (RM::PQKeys, "validate") => Signature::fixed(vec![PqKey], Bool),
        (RM::PQKeys, "verify_dilithium") => Signature::fixed(vec![PqKey, Hash, Bytes], Bool),
        (RM::PQKeys, "hybrid_verify") => Signature::fixed(vec![PqKey, Bytes, Hash], Bool),
        (RM::PQKeys, "encapsulate") => Signature::fixed(vec![Bytes], Bytes),
        (RM::PQKeys, "rand_pq") => Signature::fixed(vec![Bytes], Hash),

        // -------- EncryptedTokens --------
        (RM::EncryptedTokens, "encrypt_with") => {
            Signature::fixed(vec![PqKey, Amount], Ciphertext(Box::new(Amount)))
        }
        (RM::EncryptedTokens, "add_encrypted") => Signature::fixed(
            vec![Ciphertext(Box::new(Amount)), Ciphertext(Box::new(Amount))],
            Ciphertext(Box::new(Amount)),
        ),
        (RM::EncryptedTokens, "compare_encrypted") => Signature::fixed(
            vec![Ciphertext(Box::new(Amount)), Ciphertext(Box::new(Amount))],
            Ciphertext(Box::new(Bool)),
        ),
        (RM::EncryptedTokens, "serialize") => {
            Signature::fixed(vec![Ciphertext(Box::new(Unknown))], Bytes)
        }
        (RM::EncryptedTokens, "deserialize") => {
            Signature::fixed(vec![Bytes], Ciphertext(Box::new(Unknown)))
        }
        (RM::EncryptedTokens, "key_switch") => Signature::fixed(
            vec![Ciphertext(Box::new(Unknown)), Hash, Bytes],
            Ciphertext(Box::new(Unknown)),
        ),

        // -------- FHEVerification --------
        (RM::FHEVerification, "verify") => Signature::fixed(vec![Bytes, Hash, Bytes], Bool),
        (RM::FHEVerification, "extract_outputs") => Signature::fixed(vec![Bytes, Hash], Bytes),
        (RM::FHEVerification, "verify_vdf") => {
            Signature::fixed(vec![Hash, Bytes, Bytes, Duration], Bool)
        }
        (RM::FHEVerification, "prove") => Signature::fixed(vec![Text, Bytes, Bytes], Bytes),
        (RM::FHEVerification, "aggregate") => Signature::fixed(vec![List(Box::new(Bytes))], Bytes),

        // -------- Amnesia --------
        (RM::Amnesia, "begin_destruction") => Signature::fixed(vec![PqKey], Hash),
        (RM::Amnesia, "submit_share") => Signature::fixed(vec![Hash, Bytes], Unit),
        (RM::Amnesia, "finalize_destruction") => Signature::fixed(vec![Hash], Bool),
        (RM::Amnesia, "reconstruct") => Signature::fixed(vec![List(Box::new(Bytes)), Hash], Bytes),
        (RM::Amnesia, "verify_share") => Signature::fixed(vec![Bytes, Hash, Amount], Bool),

        // -------- Math --------
        (RM::Math, "min") | (RM::Math, "max") => Signature::fixed(vec![Amount, Amount], Amount),
        (RM::Math, "abs") => Signature::fixed(vec![Amount], Amount),
        (RM::Math, "pow") => Signature::fixed(vec![Amount, Amount], Amount),
        (RM::Math, "sqrt") => Signature::fixed(vec![Amount], Amount),
        (RM::Math, "sum") => Signature::fixed(vec![List(Box::new(Amount))], Amount),
        (RM::Math, "avg") => Signature::fixed(vec![List(Box::new(Amount))], Amount),
        (RM::Math, "median") => Signature::fixed(vec![List(Box::new(Amount))], Amount),

        // -------- Crypto --------
        (RM::Crypto, "keccak") => Signature {
            params: vec![Bytes],
            ret: Hash,
            variadic: VariadicKind::AnyHashable,
        },
        (RM::Crypto, "sha256") | (RM::Crypto, "blake2b") => Signature::fixed(vec![Bytes], Hash),
        (RM::Crypto, "poseidon") => Signature::fixed(vec![List(Box::new(Amount))], Hash),
        (RM::Crypto, "verify_ecdsa") => Signature::fixed(vec![Hash, Bytes, Address], Bool),
        (RM::Crypto, "verify_ed25519") => Signature::fixed(vec![Hash, Bytes, Bytes], Bool),
        (RM::Crypto, "hmac") => Signature::fixed(vec![Bytes, Bytes], Hash),

        // -------- Encoding --------
        (RM::Encoding, "encode") => Signature {
            params: vec![Unknown],
            ret: Bytes,
            variadic: VariadicKind::AnyHashable,
        },
        (RM::Encoding, "decode") => Signature::fixed(vec![Bytes], Unknown),
        (RM::Encoding, "rlp_encode") => Signature {
            params: vec![Unknown],
            ret: Bytes,
            variadic: VariadicKind::AnyHashable,
        },
        (RM::Encoding, "rlp_decode") => Signature::fixed(vec![Bytes], Unknown),
        (RM::Encoding, "abi_pack") => Signature {
            params: vec![Unknown],
            ret: Bytes,
            variadic: VariadicKind::AnyHashable,
        },

        // -------- Testing (test-only) --------
        (RM::Testing, "decrypt_in_test") => {
            Signature::fixed(vec![Ciphertext(Box::new(Unknown))], Unknown)
        }
        (RM::Testing, "as_caller") => Signature::fixed(vec![Address], Unit),
        (RM::Testing, "expect_revert") => Signature::fixed(vec![Unknown], Unit),
        (RM::Testing, "expect_revert_containing") => Signature::fixed(vec![Text], Unit),
        (RM::Testing, "measure_gas") => Signature::fixed(vec![], Unknown),
        (RM::Testing, "random_address") => Signature::fixed(vec![], Address),
        (RM::Testing, "advance_time") => Signature::fixed(vec![Duration], Unit),
        (RM::Testing, "deploy") => Signature::fixed(vec![Text, List(Box::new(Bytes))], Address),

        _ => return None,
    };
    Some(sig)
}

/// Map resolver's `StdlibModule` tag to the type system's equivalent.
pub fn module_tag(m: StdlibModule) -> StdlibModuleTag {
    use StdlibModule as S;
    use StdlibModuleTag as T;
    match m {
        S::PQKeys => T::PQKeys,
        S::EncryptedTokens => T::EncryptedTokens,
        S::FHEVerification => T::FHEVerification,
        S::Amnesia => T::Amnesia,
        S::Math => T::Math,
        S::Crypto => T::Crypto,
        S::Encoding => T::Encoding,
        S::Testing => T::Testing,
    }
}

pub fn module_from_tag(t: StdlibModuleTag) -> StdlibModule {
    use StdlibModule as S;
    use StdlibModuleTag as T;
    match t {
        T::PQKeys => S::PQKeys,
        T::EncryptedTokens => S::EncryptedTokens,
        T::FHEVerification => S::FHEVerification,
        T::Amnesia => S::Amnesia,
        T::Math => S::Math,
        T::Crypto => S::Crypto,
        T::Encoding => S::Encoding,
        T::Testing => S::Testing,
    }
}
