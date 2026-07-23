//! Top-level IR module container.

use std::collections::HashMap;

use covenant_diag::{SourceId, Span};
use covenant_lexer::DurationUnit;
use covenant_parser::ast::{ConstructKind, Ident, Principal, PrivacyQualifier};
use covenant_privacy::PrivacyDomain;
use covenant_types::Ty;

use crate::function::IrFunction;
use crate::id::{FunctionId, GlobalId, StructTypeId};

#[derive(Debug, Clone)]
pub struct IrModule {
    pub source_id: SourceId,
    pub name: Ident,
    pub construct_kind: ConstructKind,
    pub construct_privacy: Option<PrivacyQualifier>,

    pub fields: Vec<IrField>,
    pub structs: Vec<IrStruct>,
    pub errors: Vec<IrError>,
    pub events: Vec<IrEvent>,
    pub choices: Vec<IrChoice>,

    pub functions: Vec<IrFunction>,
    pub external_contracts: Vec<IrExternalContract>,

    pub metadata: HashMap<Box<str>, IrMetadataValue>,
    pub anchor: Option<AnchorInfo>,
    pub upgradeable: Option<UpgradeableInfo>,
}

/// An external Solidity contract interface registered in the IR.
#[derive(Debug, Clone)]
pub struct IrExternalContract {
    pub name: Ident,
    pub functions: Vec<IrExternalFunc>,
}

/// A single external function entry.
/// The 4-byte selector is computed from `abi_sig` at EVM codegen time.
#[derive(Debug, Clone)]
pub struct IrExternalFunc {
    pub name: Ident,
    /// Canonical ABI signature, e.g. `"transfer(address,uint256)"`.
    pub abi_sig: Box<str>,
    pub is_view: bool,
    pub param_count: u32,
}

#[derive(Debug, Clone)]
pub struct IrField {
    pub id: GlobalId,
    pub name: Ident,
    pub ty: Ty,
    pub privacy: PrivacyDomain,
    pub initializer_fn: Option<FunctionId>,
    pub initializer_const: Option<crate::instr::IrConstant>,
    pub span: Span,
    /// Explicit storage slot from `@slot(N)` annotation (KSR-CVN-021).
    /// `None` → sequential layout. When `Some(n)`, codegen places the field
    /// at slot `n` instead of the next sequential slot.
    pub explicit_slot: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct IrStruct {
    pub id: StructTypeId,
    pub name: Ident,
    pub fields: Vec<IrStructField>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrStructField {
    pub name: Ident,
    pub ty: Ty,
    pub indexed: bool,
}

#[derive(Debug, Clone)]
pub struct IrError {
    pub name: Ident,
    pub params: Vec<Ty>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrEvent {
    pub name: Ident,
    pub params: Vec<(Ident, Ty, bool)>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrChoice {
    pub members: Vec<Box<str>>,
    pub is_inline: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum IrMetadataValue {
    Integer(u128),
    Text(Box<str>),
    Array(Vec<IrMetadataValue>),
    Duration { n: u64, unit: DurationUnit },
    Address(Box<[u8; 20]>),
    Hex(Box<[u8]>),
    Bool(bool),
    GenesisMint { amount: u128, to: Principal },
}

#[derive(Debug, Clone)]
pub struct AnchorInfo {
    pub chains: Vec<Box<str>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct UpgradeableInfo {
    pub principal: Principal,
    pub span: Span,
}
