//! Abstract syntax tree for Covenant source files.
//!
//! Naming matches Doc 5 §6 and Phase 2 spec; do not rename without updating both.
//!
//! Invariants:
//! * every node carries (or transitively contains) a `Span`;
//! * the tree is immutable after construction, downstream passes build parallel
//!   structures keyed by AST nodes rather than mutating them.

use covenant_diag::{SourceId, Span};
use covenant_lexer::DurationUnit;

// ======================================================================
// File and top-level
// ======================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub source_id: SourceId,
    pub header: Option<FileHeader>,
    pub imports: Vec<Import>,
    pub external_contracts: Vec<ExternalContractDecl>,
    pub top_level: TopLevel,
}

/// `external contract IERC20 { function transfer(address, amount) view }`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalContractDecl {
    pub name: Ident,
    pub functions: Vec<ExternalFunctionDecl>,
    pub span: Span,
}

/// A single function signature inside an `external contract` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalFunctionDecl {
    pub name: Ident,
    pub params: Vec<ExternalParam>,
    pub returns: Option<Type>,
    pub is_view: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalParam {
    pub name: Option<Ident>,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHeader {
    pub covenant_version: u32,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub path: Vec<Ident>,
    pub alias: Option<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopLevel {
    pub span: Span,
    pub privacy: Option<PrivacyQualifier>,
    pub keyword: ConstructKind,
    pub name: Ident,
    pub anchor: Option<AnchorClause>,
    pub upgradeable: Option<UpgradeableClause>,
    pub body: Vec<TopLevelDecl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructKind {
    Module,
    Record,
    Token,
    Ballot,
    Counter,
    Board,
    Market,
    Vault,
    Registry,
    Bridge,
    Ceremony,
    Nft,
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyQualifier {
    Public,
    Private,
    Encrypted,
    Sealed,
    Hybrid,
    Confidential,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorClause {
    pub chains: Vec<TextLit>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeableClause {
    pub principal: Principal,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: Box<str>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLit {
    pub value: Box<str>,
    pub span: Span,
}

// ======================================================================
// Principals
// ======================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    Named(PrincipalKind),
    Predicate(Ident),
    Address(Expr),
    Call { name: Ident, args: Vec<Expr> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalKind {
    Owner,
    Deployer,
    Admin,
    Caller,
    Guardians,
    Parties,
    Holders,
}

// ======================================================================
// Top-level declarations
// ======================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopLevelDecl {
    Metadata(MetadataAssignment),
    Field(FieldDecl),
    Struct(StructDecl),
    Credential(CredentialDecl),
    Error(ErrorDecl),
    Event(EventDecl),
    Action(ActionDecl),
    View(ViewDecl),
    Reveal(RevealDecl),
    Version(VersionBlock),
    Migrate(MigrateBlock),
    OnDestroy(OnDestroyBlock),
    SelectiveDisclosure(SelectiveDisclosureDecl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataAssignment {
    pub name: Ident,
    pub value: MetadataValue,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataValue {
    Literal(Expr),
    GenesisMint { amount: Expr, to: Principal },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    pub span: Span,
    pub explicit_field_keyword: bool,
    pub per_field_privacy: Option<PrivacyQualifier>,
    pub name: Ident,
    pub ty: Type,
    pub initializer: Option<Expr>,
    /// Annotations attached to this field (KSR-CVN-021). Currently used for
    /// `@slot(N)` to pin storage layout for upgradeable contracts.
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDecl {
    pub name: Ident,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialDecl {
    pub name: Ident,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorDecl {
    pub name: Ident,
    pub fields: Vec<ErrorField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorField {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDecl {
    pub name: Ident,
    pub args: Vec<EventArg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventArg {
    pub name: Ident,
    pub ty: Type,
    pub indexed: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDecl {
    pub span: Span,
    pub annotations: Vec<Annotation>,
    pub name: Ident,
    pub args: Vec<Arg>,
    pub guards: Vec<Guard>,
    pub qualifiers: Vec<ActionQualifier>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewDecl {
    pub span: Span,
    pub name: Ident,
    pub args: Vec<Arg>,
    pub returns: Option<Type>,
    pub guards: Vec<Guard>,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealDecl {
    pub span: Span,
    pub name: Ident,
    pub args: Vec<Arg>,
    pub returns: Option<Type>,
    pub target: Option<RevealTarget>,
    pub guards: Vec<Guard>,
    /// `None` for the `reveal <name> to <principal>` one-liner form.
    pub body: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guard {
    When(Expr),
    Only(Principal),
    Given(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionQualifier {
    VerifiedBy(Expr),
    PqSigned {
        msg: Expr,
        sig: Expr,
        pk: Expr,
        span: Span,
    },
    VdfLocked {
        delay: Expr,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub name: Ident,
    pub args: Vec<AnnotationArg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationArg {
    Positional(Expr),
    Named { name: Ident, value: Expr },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevealTarget {
    Owner,
    Caller,
    Parties,
    Address(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionBlock {
    pub number: u32,
    pub body: Vec<TopLevelDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateBlock {
    pub from: u32,
    pub to: u32,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnDestroyBlock {
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectiveDisclosureDecl {
    pub name: Ident,
    pub args: Vec<Arg>,
    pub circuit_name: Ident,
    pub body: Expr,
    pub span: Span,
}

// ======================================================================
// Types
// ======================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Amount(Span),
    Time(Span),
    Duration(Span),
    Hash(Span),
    Text(Span),
    Address(Span),
    Choice(Span, Vec<ChoiceValue>),
    PqKey(Span),
    Bytes(Span),
    Bool(Span),
    Encrypted(Box<Type>, Span),
    List(Box<Type>, Span),
    Map(Box<Type>, Box<Type>, Span),
    PriorityQueue(Box<Type>, Box<Type>, Ordering, Span),
    Shares {
        n: u32,
        k: u32,
        parties: Box<Expr>,
        span: Span,
    },
    User(Ident),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceValue {
    pub name: Ident,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordering {
    Max,
    Min,
}

// ======================================================================
// Statements
// ======================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let {
        name: Ident,
        ty: Option<Type>,
        value: Expr,
        span: Span,
    },
    Assign {
        target: LValue,
        op: AssignOp,
        value: Expr,
        span: Span,
    },
    Expr(Expr, Span),
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
        span: Span,
    },
    If {
        cond: Expr,
        then_block: Vec<Stmt>,
        else_ifs: Vec<(Expr, Vec<Stmt>)>,
        else_block: Option<Vec<Stmt>>,
        span: Span,
    },
    EncryptedWhen {
        cond: Expr,
        then_block: Vec<Stmt>,
        otherwise: Option<Vec<Stmt>>,
        span: Span,
    },
    ForEach {
        binding: Ident,
        iter: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    TryCatch {
        body: Vec<Stmt>,
        error_binding: Option<Ident>,
        catch_body: Vec<Stmt>,
        span: Span,
    },
    Emit {
        event: Ident,
        args: Vec<Expr>,
        span: Span,
    },
    Transfer {
        amount: Expr,
        from: Option<Expr>,
        to: Expr,
        span: Span,
    },
    Append {
        collection: Ident,
        struct_lit: Vec<FieldAssignment>,
        span: Span,
    },
    Discard(LValue, Span),
    Delete(LValue, Span),
    Return(Option<Expr>, Span),
    RevertWith {
        error: Ident,
        args: Vec<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LValue {
    Ident(Ident),
    FieldAccess(Box<LValue>, Ident),
    Index(Box<LValue>, Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: MatchBody,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {
    Literal(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchBody {
    Stmt(Box<Stmt>),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldAssignment {
    pub name: Ident,
    pub value: Expr,
    pub span: Span,
}

// ======================================================================
// Expressions
// ======================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(LiteralExpr),
    Ident(Ident),
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    FieldAccess {
        base: Box<Expr>,
        field: Ident,
        span: Span,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Slice {
        base: Box<Expr>,
        from: Box<Expr>,
        to: Box<Expr>,
        span: Span,
    },
    Array {
        elements: Vec<Expr>,
        span: Span,
    },
    Paren(Box<Expr>),
    If {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        span: Span,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchExprArm>,
        span: Span,
    },
    EncryptedLit(Box<Expr>, Span),
    Lambda {
        param: Ident,
        body: Box<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchExprArm {
    pub pattern: MatchPattern,
    pub body: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralExpr {
    Integer(u128, Span),
    Hex(Box<[u8]>, Span),
    Text(Box<str>, Span),
    Bool(bool, Span),
    /// A duration literal. The `u64` is the value in **seconds** (`7 days`
    /// carries 604800) and the `DurationUnit` is the unit as written, kept for
    /// rendering only. Lowering must use the `u64` as-is: re-applying the unit
    /// would scale the value twice. See `covenant_lexer::TokenKind::Duration`.
    Duration(u64, DurationUnit, Span),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Concat,
    In,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
    BitNot,
}

impl Expr {
    /// Returns the span of this expression node.
    pub fn span(&self) -> Span {
        match self {
            Expr::Literal(l) => l.span(),
            Expr::Ident(i) => i.span,
            Expr::Binary { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Call { span, .. }
            | Expr::FieldAccess { span, .. }
            | Expr::Index { span, .. }
            | Expr::Slice { span, .. }
            | Expr::Array { span, .. }
            | Expr::If { span, .. }
            | Expr::Match { span, .. }
            | Expr::Lambda { span, .. } => *span,
            Expr::EncryptedLit(_, span) => *span,
            Expr::Paren(inner) => inner.span(),
        }
    }
}

impl LiteralExpr {
    pub fn span(&self) -> Span {
        match self {
            LiteralExpr::Integer(_, s)
            | LiteralExpr::Hex(_, s)
            | LiteralExpr::Text(_, s)
            | LiteralExpr::Bool(_, s)
            | LiteralExpr::Duration(_, _, s) => *s,
        }
    }
}
