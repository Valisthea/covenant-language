//! Privacy domain lattice and per-statement flow-context stack.
//!
//! `PrivacyDomain` is the runtime observability of a value. `FlowContext`
//! tracks whether we're inside an encrypted conditional branch (which makes
//! side effects leak 1 bit per branch traversal).

use covenant_types::Ty;

/// The observability domain of a value. Derived from its `Ty`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivacyDomain {
    /// Observable on-chain as plaintext.
    Plaintext,
    /// Observable only as ciphertext: requires threshold decryption or a
    /// zero-knowledge proof to see the underlying value.
    Encrypted,
    /// Domain could not be determined (type was `Unknown`). Treated as
    /// compatible with anything to suppress cascading diagnostics.
    Unknown,
}

/// Map a type to its privacy domain.
///
/// Composite types (`List<T>`, `Map<K, V>`) are structurally Plaintext, the
/// shape is public: but element access rederives the domain of the element
/// type. The analyzer records domains per expression span, so `xs[i]` ends up
/// Encrypted even though the `xs` expression itself is Plaintext.
pub fn domain_of(ty: &Ty) -> PrivacyDomain {
    match ty {
        Ty::Ciphertext(_) => PrivacyDomain::Encrypted,
        Ty::Unknown => PrivacyDomain::Unknown,
        _ => PrivacyDomain::Plaintext,
    }
}

/// Statement-level flow context. Pushed/popped as the analyzer descends into
/// nested blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowContext {
    /// Ordinary statement flow.
    Normal,
    /// Inside the then-branch of `encrypted_when { ... }`. Side effects here
    /// leak one bit: the fact that the encrypted condition was true.
    EncryptedThen,
    /// Inside the otherwise-branch. Same leak profile as `EncryptedThen`.
    EncryptedOtherwise,
    /// Inside an `on_destroy` block. Privileged intrinsics (`destroy`,
    /// `freeze`) are allowed.
    OnDestroy,
    /// Inside a `selective_disclosure` circuit body. Expressions here become
    /// constraints rather than runtime code.
    SelectiveDisclosure,
}

impl FlowContext {
    pub fn is_encrypted_branch(self) -> bool {
        matches!(self, Self::EncryptedThen | Self::EncryptedOtherwise)
    }
}
