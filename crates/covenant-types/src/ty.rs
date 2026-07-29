//! Internal type representation (`Ty`) used by the Covenant type checker.
//!
//! See Doc 5 §8.2 and Doc 2 §3. Nominal types (`Struct`, `Credential`, `Choice`)
//! are stored as opaque IDs indexing into the `TypeTable`'s registries.

/// Max/min ordering parameter of `priority_queue<K, V, ord>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ordering {
    Max,
    Min,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChoiceTypeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CredentialId(pub u32);

/// Stdlib module reference reflected into the type system so
/// `PQKeys.verify_dilithium(...)` can dispatch on the base's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StdlibModuleTag {
    PQKeys,
    EncryptedTokens,
    FHEVerification,
    Amnesia,
    Math,
    Crypto,
    Encoding,
    Testing,
}

/// Internal type. Every typed AST expression maps to one of these.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // --- Primitives ---
    Amount,
    Time,
    Duration,
    Hash,
    Text,
    Address,
    Bool,
    Bytes,
    PqKey,

    // --- Nominal ---
    Choice(ChoiceTypeId),
    Struct(StructId),
    Credential(CredentialId),

    // --- Composite ---
    Ciphertext(Box<Ty>),
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    PriorityQueue {
        key: Box<Ty>,
        value: Box<Ty>,
        ordering: Ordering,
    },
    Shares {
        n: u32,
        k: u32,
        parties_ty: Box<Ty>,
    },

    // --- Callable shape (stdlib fns, actions, views, reveals, methods) ---
    Fn {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },

    // --- Stdlib module (before method access) ---
    StdlibModule(StdlibModuleTag),

    /// Namespace marker: distinguishes `Msg` / `Block` / `Phase` from ordinary
    /// types so `msg.value` / `block.timestamp` can dispatch via a parallel table.
    Namespace(Namespace),

    // --- No-value statement context ---
    Unit,

    /// Error sentinel. Unknown is compatible with any expected type; the
    /// typechecker uses it to suppress cascading diagnostics downstream of an
    /// existing error.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    Msg,
    Block,
}

impl Ty {
    /// Internal compatibility check used inside the checker. `Unknown` on
    /// either side is treated as compatible (error suppression).
    pub fn compatible(&self, other: &Ty) -> bool {
        matches!(self, Ty::Unknown) || matches!(other, Ty::Unknown) || self == other
    }

    pub fn is_ciphertext(&self) -> bool {
        matches!(self, Ty::Ciphertext(_))
    }

    /// If this type is `Ciphertext(T)`, return `T`. Otherwise `None`.
    pub fn strip_ciphertext(&self) -> Option<&Ty> {
        match self {
            Ty::Ciphertext(t) => Some(t),
            _ => None,
        }
    }

    /// Construct `Ciphertext(self)` unless `self` is already a ciphertext.
    pub fn wrap_ciphertext(self) -> Ty {
        match self {
            Ty::Ciphertext(_) => self,
            other => Ty::Ciphertext(Box::new(other)),
        }
    }

    pub fn is_eq_comparable(&self) -> bool {
        use Ty::*;
        matches!(
            self,
            Amount
                | Time
                | Duration
                | Hash
                | Address
                | Bool
                | Text
                | Bytes
                | Choice(_)
                | PqKey
                | Struct(_)
                | Credential(_)
        )
    }

    pub fn is_ord_comparable(&self) -> bool {
        matches!(self, Ty::Amount | Ty::Time | Ty::Duration)
    }

    /// Human-readable rendering for diagnostics.
    pub fn render(&self, t: &crate::table::TypeTable) -> String {
        match self {
            Ty::Amount => "amount".into(),
            Ty::Time => "time".into(),
            Ty::Duration => "duration".into(),
            Ty::Hash => "hash".into(),
            Ty::Text => "text".into(),
            Ty::Address => "address".into(),
            Ty::Bool => "bool".into(),
            Ty::Bytes => "bytes".into(),
            Ty::PqKey => "pq_key".into(),
            Ty::Choice(id) => t
                .choices
                .get(id.0 as usize)
                .map(|c| {
                    if c.is_inline {
                        format!("choice {:?}", c.members)
                    } else {
                        format!("choice `{}`", c.name.as_deref().unwrap_or("?"))
                    }
                })
                .unwrap_or_else(|| format!("choice#{}", id.0)),
            Ty::Struct(id) => t
                .structs
                .get(id.0 as usize)
                .map(|s| format!("struct `{}`", s.name.name.as_ref()))
                .unwrap_or_else(|| format!("struct#{}", id.0)),
            Ty::Credential(id) => t
                .credentials
                .get(id.0 as usize)
                .map(|c| format!("credential `{}`", c.name.name.as_ref()))
                .unwrap_or_else(|| format!("credential#{}", id.0)),
            Ty::Ciphertext(inner) => format!("ciphertext<{}>", inner.render(t)),
            Ty::List(inner) => format!("[{}]", inner.render(t)),
            Ty::Map(k, v) => format!("map<{}, {}>", k.render(t), v.render(t)),
            Ty::PriorityQueue {
                key,
                value,
                ordering,
            } => format!(
                "priority_queue<{}, {}, {:?}>",
                key.render(t),
                value.render(t),
                ordering
            ),
            Ty::Shares { n, k, .. } => format!("shares({} of {} among ...)", n, k),
            Ty::Fn { params, ret } => {
                let ps = params
                    .iter()
                    .map(|p| p.render(t))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({}) -> {}", ps, ret.render(t))
            }
            Ty::StdlibModule(m) => format!("stdlib::{m:?}"),
            Ty::Namespace(ns) => format!("<{ns:?}>"),
            Ty::Unit => "()".into(),
            Ty::Unknown => "<unknown>".into(),
        }
    }
}
