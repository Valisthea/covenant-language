//! Binding types: the kinds of things an identifier in the AST can refer to.
//!
//! Every `Ident` that survives Phase 3 is bound to a `Binding`. Unresolved
//! identifiers are bound to `Binding::Unresolved` so that downstream phases
//! don't cascade errors on the same name.

/// Identifier for a top-level declaration (field, struct, action, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeclId(pub u32);

/// Identifier for a local binding (let, argument, for-each, try-catch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32);

/// What an identifier refers to after name resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    // --- User-declared top-level decls ---
    Field(DeclId),
    Struct(DeclId),
    Credential(DeclId),
    Error(DeclId),
    Event(DeclId),
    Action(DeclId),
    View(DeclId),
    Reveal(DeclId),
    SelectiveDisclosure(DeclId),

    /// Function argument, let-binding, for-each binding, try-catch binding.
    Local(LocalId),

    /// Language-provided identifier (caller, now, deployer, owner, ...).
    LangIdent(LangIdent),

    /// Compiler-recognized predicate usable as a principal
    /// (`first_time_caller`, `registered_key`, `validator_majority(...)`, ...).
    BuiltinPredicate(BuiltinPredicate),

    /// Stdlib module name (PQKeys, EncryptedTokens, Math, ...). Method lookup
    /// inside the module is deferred to the type checker.
    StdlibModule(StdlibModule),

    /// Stdlib free function (keccak, encode, encrypted, destroy, ...).
    StdlibFn(StdlibFn),

    /// External contract interface declared via `external contract IFoo
    /// { function method(args) ... }`. Method lookup on `IFoo.at(addr)`
    /// chains is deferred to the type checker (V0.9.1 surfaces the
    /// binding ; deeper signature/return-type checking is best-effort
    /// and falls back to bytecode trust).
    ExternalContract(DeclId),

    /// Resolution failed. Sentinel that suppresses cascading errors.
    Unresolved,
}

/// Language-provided context-dependent identifiers.
///
/// See Doc 2 §1.5.8 and Doc 5 §7.4. The catalog below extends Doc 2's list with
/// `Owner` (which the Codex Basics treat as language-provided) and a handful of
/// construct-implicit fields (`Tally`, `Posts`, `OpenedAt`) used by ballot and
/// board examples. Flagged as spec ambiguities in the Phase 3 report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangIdent {
    // Any behavior scope
    Caller,
    Now,
    CurrentBlock,
    Block,
    Msg,

    // Any construct scope
    Deployer,
    This,
    ZeroAddress,
    /// `owner` — either the user's `owner: address` field if declared, or the
    /// implicit deployer-by-convention binding.
    Owner,

    // Only in bridges
    ThisChain,
    OtherChain,

    // Only in ceremonies
    Phase,

    // Only in actions with `verified_by(...)` qualifier
    ProofPayload,

    // Only in sealed markets / ballots / ceremonies / vaults
    OpensAt,
    ClosesAt,
    /// Alias for `opens_at` used in Codex Basics OpenBallot ("opened_at").
    OpenedAt,

    // MPC-style parties context
    Party,
    Parties,
    Guardians,
    Admin,
    Holders,

    // Construct-implicit fields (ballot `tally`, board `posts`)
    Tally,
    Posts,
}

impl LangIdent {
    /// Canonical spelling of this identifier as it appears in source code.
    pub fn name(self) -> &'static str {
        match self {
            LangIdent::Caller => "caller",
            LangIdent::Now => "now",
            LangIdent::CurrentBlock => "current_block",
            LangIdent::Block => "block",
            LangIdent::Msg => "msg",
            LangIdent::Deployer => "deployer",
            LangIdent::This => "this",
            LangIdent::ZeroAddress => "zero_address",
            LangIdent::Owner => "owner",
            LangIdent::ThisChain => "this_chain",
            LangIdent::OtherChain => "other_chain",
            LangIdent::Phase => "phase",
            LangIdent::ProofPayload => "proof_payload",
            LangIdent::OpensAt => "opens_at",
            LangIdent::ClosesAt => "closes_at",
            LangIdent::OpenedAt => "opened_at",
            LangIdent::Party => "party",
            LangIdent::Parties => "parties",
            LangIdent::Guardians => "guardians",
            LangIdent::Admin => "admin",
            LangIdent::Holders => "holders",
            LangIdent::Tally => "tally",
            LangIdent::Posts => "posts",
        }
    }
}

/// Compiler-recognized principal predicates that can appear in `only <pred>`
/// guards and similar positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinPredicate {
    FirstTimeCaller,
    RegisteredKey,
    LosingBidder,
    KnownIssuer,
    Party,
    AnyoneAfterGrace,
    ValidVdfProof,
    NotDeposited,
    NotFunded,
    NotReleased,
    NotDrawn,
    NotYetRevealed,
    Funded,
    Released,
    Drawn,
    Deposited,
    /// Takes an argument list: `validator_majority(sigs)`.
    ValidatorMajority,
}

impl BuiltinPredicate {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "first_time_caller" => Self::FirstTimeCaller,
            "registered_key" => Self::RegisteredKey,
            "losing_bidder" => Self::LosingBidder,
            "known_issuer" => Self::KnownIssuer,
            "party" => Self::Party,
            "anyone_after_grace" => Self::AnyoneAfterGrace,
            "valid_vdf_proof" => Self::ValidVdfProof,
            "not_deposited" => Self::NotDeposited,
            "not_funded" => Self::NotFunded,
            "not_released" => Self::NotReleased,
            "not_drawn" => Self::NotDrawn,
            "not_yet_revealed" => Self::NotYetRevealed,
            "funded" => Self::Funded,
            "released" => Self::Released,
            "drawn" => Self::Drawn,
            "deposited" => Self::Deposited,
            "validator_majority" => Self::ValidatorMajority,
            _ => return None,
        })
    }
}

/// Stdlib modules implicitly available per Doc 3 §1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdlibModule {
    PQKeys,
    EncryptedTokens,
    FHEVerification,
    Amnesia,
    Math,
    Crypto,
    Encoding,
    Testing,
}

impl StdlibModule {
    pub fn name(self) -> &'static str {
        match self {
            Self::PQKeys => "PQKeys",
            Self::EncryptedTokens => "EncryptedTokens",
            Self::FHEVerification => "FHEVerification",
            Self::Amnesia => "Amnesia",
            Self::Math => "Math",
            Self::Crypto => "Crypto",
            Self::Encoding => "Encoding",
            Self::Testing => "Testing",
        }
    }
}

/// Stdlib free functions implicitly in scope in every source file.
///
/// The test-mode subset (`DecryptInTest`, `MeasureGas`, ...) is currently
/// registered unconditionally; the type checker / driver will later validate
/// they appear only in `test` targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdlibFn {
    // Always in scope
    Encrypted,
    CiphertextHashOf,
    Keccak,
    Encode,
    Decode,
    Destroy,
    Freeze,
    RandPq,
    Min,
    Max,
    Abs,
    Pow,
    Sqrt,

    // Test-target only
    DecryptInTest,
    MeasureGas,
    ExpectRevert,
    AsCaller,
    AdvanceTime,
    Deploy,
    RandomAddress,
}

impl StdlibFn {
    pub fn name(self) -> &'static str {
        match self {
            Self::Encrypted => "encrypted",
            Self::CiphertextHashOf => "ciphertext_hash_of",
            Self::Keccak => "keccak",
            Self::Encode => "encode",
            Self::Decode => "decode",
            Self::Destroy => "destroy",
            Self::Freeze => "freeze",
            Self::RandPq => "rand_pq",
            Self::Min => "min",
            Self::Max => "max",
            Self::Abs => "abs",
            Self::Pow => "pow",
            Self::Sqrt => "sqrt",
            Self::DecryptInTest => "decrypt_in_test",
            Self::MeasureGas => "measure_gas",
            Self::ExpectRevert => "expect_revert",
            Self::AsCaller => "as_caller",
            Self::AdvanceTime => "advance_time",
            Self::Deploy => "deploy",
            Self::RandomAddress => "random_address",
        }
    }
}

/// Kinds of declarations tracked in the [`super::table::DeclInfo`] table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    Field,
    Struct,
    Credential,
    Error,
    Event,
    Action,
    View,
    Reveal,
    SelectiveDisclosure,
    /// V0.9.1 — `external contract IFoo { ... }` interface declarations.
    ExternalContract,
}

/// Kinds of local bindings tracked in the [`super::table::LocalInfo`] table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    Argument,
    Let,
    ForEachBinding,
    TryCatchBinding,
    SelectiveDisclosureArg,
}
