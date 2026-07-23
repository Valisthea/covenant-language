//! Token types for the Covenant lexer.
//!
//! Naming is normative per Phase 1 spec: every keyword is `Kw*`, every primitive
//! operator has a single canonical name. Do not rename without updating the spec.

use covenant_diag::Span;

/// A fully-resolved token emitted by the lexer, after keyword resolution and
/// duration-literal combining.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Duration literal units recognized by the lexer.
///
/// See Language Spec §1.6.5. Plural and singular forms are accepted equivalently;
/// both `1 day` and `1 days` produce `DurationUnit::Days`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Weeks,
}

/// The kind tag of a [`Token`].
///
/// Organized per Phase 1 spec into literals, identifiers, keyword groups,
/// punctuation, operators, and structural markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // --- Literals ---
    Integer(u128),
    HexBytes(Box<[u8]>),
    Text(Box<str>),
    Duration(u64, DurationUnit),
    True,
    False,

    // --- Identifier (non-keyword) ---
    Ident(Box<str>),

    // --- Top-level construct keywords ---
    KwModule,
    KwRecord,
    KwToken,
    KwBallot,
    KwCounter,
    KwBoard,
    KwMarket,
    KwVault,
    KwRegistry,
    KwBridge,
    KwCeremony,
    KwNft,
    KwTest,

    // --- Privacy qualifier keywords ---
    KwPublic,
    KwPrivate,
    KwEncrypted,
    KwSealed,
    KwHybrid,
    KwConfidential,

    // --- Behavior verb keywords ---
    KwAction,
    KwView,
    KwReveal,

    // --- Guard keywords ---
    KwWhen,
    KwOnly,
    KwGiven,

    // --- Type keywords ---
    KwAmount,
    KwTime,
    KwDuration,
    KwHash,
    KwTextTy,
    KwAddress,
    KwChoice,
    KwPqKey,
    KwBytes,
    KwBool,
    KwCiphertext,
    KwMap,
    KwPriorityQueue,
    KwShares,

    // --- Declaration keywords ---
    KwField,
    KwEvent,
    KwError,
    KwStruct,
    KwCredential,
    KwVersion,

    // --- Control flow keywords ---
    KwMatch,
    KwLet,
    KwReturn,
    KwFor,
    KwEach,
    KwIn,
    KwIf,
    KwElse,
    KwEncryptedWhen,
    KwOtherwise,
    KwTryAction,
    KwCatch,
    KwRevertWith,

    // --- Ceremony & lifecycle keywords ---
    KwOnDestroy,
    KwMigrate,
    KwFrom,
    KwTo,
    KwAnchoredOn,
    KwUpgradeableBy,
    KwSelectiveDisclosure,
    KwUsing,
    KwAs,
    KwReturns,
    KwImport,

    // --- Statement verbs ---
    KwEmit,
    KwTransfer,
    KwAppend,
    KwDiscard,
    KwDelete,

    // --- Action qualifier keywords ---
    KwVerifiedBy,
    KwPqSigned,
    KwVdfLocked,

    // --- Solidity interop keywords ---
    KwExternal,
    KwContract,
    KwFunction,

    // --- Punctuation ---
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
    Colon,
    ColonColon,
    Dot,
    DotDot,
    Arrow,
    FatArrow,
    At,
    Underscore,

    // --- Operators ---
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    Eq,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AmpAmp,
    PipePipe,
    Bang,
    Amp,
    Pipe,
    Caret,
    Tilde,
    ShiftLeft,
    ShiftRight,
    PlusPlus,

    // --- Structural ---
    Newline,
    Eof,

    // --- Error sentinel (always accompanied by a Diagnostic) ---
    Error,
}

/// Resolve a `[A-Za-z_][A-Za-z0-9_]*` identifier string to its `TokenKind`.
///
/// Covers the entire keyword set from Doc 2 §1.5 plus boolean literals.
/// Returns `TokenKind::Ident` for anything not matching a keyword.
pub fn resolve_keyword(s: &str) -> TokenKind {
    match s {
        // Boolean literals
        "true" => TokenKind::True,
        "false" => TokenKind::False,

        // Top-level construct keywords
        "module" => TokenKind::KwModule,
        "record" => TokenKind::KwRecord,
        "token" => TokenKind::KwToken,
        "ballot" => TokenKind::KwBallot,
        "counter" => TokenKind::KwCounter,
        "board" => TokenKind::KwBoard,
        "market" => TokenKind::KwMarket,
        "vault" => TokenKind::KwVault,
        "registry" => TokenKind::KwRegistry,
        "bridge" => TokenKind::KwBridge,
        "ceremony" => TokenKind::KwCeremony,
        "nft" => TokenKind::KwNft,
        "test" => TokenKind::KwTest,

        // Privacy qualifiers
        "public" => TokenKind::KwPublic,
        "private" => TokenKind::KwPrivate,
        "encrypted" => TokenKind::KwEncrypted,
        "sealed" => TokenKind::KwSealed,
        "hybrid" => TokenKind::KwHybrid,
        "confidential" => TokenKind::KwConfidential,

        // Behavior verbs
        "action" => TokenKind::KwAction,
        "view" => TokenKind::KwView,
        "reveal" => TokenKind::KwReveal,

        // Guards
        "when" => TokenKind::KwWhen,
        "only" => TokenKind::KwOnly,
        "given" => TokenKind::KwGiven,

        // Types
        "amount" => TokenKind::KwAmount,
        "time" => TokenKind::KwTime,
        "duration" => TokenKind::KwDuration,
        "hash" => TokenKind::KwHash,
        "text" => TokenKind::KwTextTy,
        "address" => TokenKind::KwAddress,
        "choice" => TokenKind::KwChoice,
        "pq_key" => TokenKind::KwPqKey,
        "bytes" => TokenKind::KwBytes,
        "bool" => TokenKind::KwBool,
        "ciphertext" => TokenKind::KwCiphertext,
        "map" => TokenKind::KwMap,
        "priority_queue" => TokenKind::KwPriorityQueue,
        "shares" => TokenKind::KwShares,

        // Declarations
        "field" => TokenKind::KwField,
        "event" => TokenKind::KwEvent,
        "error" => TokenKind::KwError,
        "struct" => TokenKind::KwStruct,
        "credential" => TokenKind::KwCredential,
        "version" => TokenKind::KwVersion,

        // Control flow
        "match" => TokenKind::KwMatch,
        "let" => TokenKind::KwLet,
        "return" => TokenKind::KwReturn,
        "for" => TokenKind::KwFor,
        "each" => TokenKind::KwEach,
        "in" => TokenKind::KwIn,
        "if" => TokenKind::KwIf,
        "else" => TokenKind::KwElse,
        "encrypted_when" => TokenKind::KwEncryptedWhen,
        "otherwise" => TokenKind::KwOtherwise,
        "try_action" => TokenKind::KwTryAction,
        "catch" => TokenKind::KwCatch,
        "revert_with" => TokenKind::KwRevertWith,

        // Ceremony / lifecycle
        "on_destroy" => TokenKind::KwOnDestroy,
        "migrate" => TokenKind::KwMigrate,
        "from" => TokenKind::KwFrom,
        "to" => TokenKind::KwTo,
        "anchored_on" => TokenKind::KwAnchoredOn,
        "upgradeable_by" => TokenKind::KwUpgradeableBy,
        "selective_disclosure" => TokenKind::KwSelectiveDisclosure,
        "using" => TokenKind::KwUsing,
        "as" => TokenKind::KwAs,
        "returns" => TokenKind::KwReturns,
        "import" => TokenKind::KwImport,

        // Statement verbs
        "emit" => TokenKind::KwEmit,
        "transfer" => TokenKind::KwTransfer,
        "append" => TokenKind::KwAppend,
        "discard" => TokenKind::KwDiscard,
        "delete" => TokenKind::KwDelete,

        // Action qualifiers
        "verified_by" => TokenKind::KwVerifiedBy,
        "pq_signed" => TokenKind::KwPqSigned,
        "vdf_locked" => TokenKind::KwVdfLocked,

        // Solidity interop
        "external" => TokenKind::KwExternal,
        "contract" => TokenKind::KwContract,
        "function" => TokenKind::KwFunction,

        // Bare underscore (identifier-shaped but lexically distinct)
        "_" => TokenKind::Underscore,

        // Everything else is a user identifier
        _ => TokenKind::Ident(s.into()),
    }
}

/// Matches an identifier against the set of duration unit words.
pub fn match_duration_unit(s: &str) -> Option<DurationUnit> {
    match s {
        "second" | "seconds" => Some(DurationUnit::Seconds),
        "minute" | "minutes" => Some(DurationUnit::Minutes),
        "hour" | "hours" => Some(DurationUnit::Hours),
        "day" | "days" => Some(DurationUnit::Days),
        "week" | "weeks" => Some(DurationUnit::Weeks),
        _ => None,
    }
}
