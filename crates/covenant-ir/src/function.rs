//! Function (control-flow graph) and parameter/guard/qualifier types.

use std::collections::HashMap;

use covenant_diag::Span;
use covenant_parser::ast::Ident;
use covenant_privacy::PrivacyDomain;
use covenant_resolver::{BuiltinPredicate, LocalId};
use covenant_types::Ty;

use crate::block::IrBlock;
use crate::id::{BlockId, FunctionId, GlobalId, Value};
use crate::instr::ValueInfo;

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub id: FunctionId,
    pub name: Ident,
    pub kind: IrFunctionKind,
    pub params: Vec<IrParam>,
    pub returns: Option<Ty>,
    pub guards: Vec<IrGuard>,
    pub qualifiers: Vec<IrActionQualifier>,
    pub annotations: Vec<IrAnnotation>,

    pub blocks: Vec<IrBlock>,
    pub entry: BlockId,

    pub values: Vec<(Value, ValueInfo)>,
    pub value_types: HashMap<Value, Ty>,
    pub value_privacy: HashMap<Value, PrivacyDomain>,
    pub local_to_value: HashMap<LocalId, Value>,
    pub value_spans: HashMap<Value, Span>,

    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrFunctionKind {
    Action,
    View,
    Reveal,
    OnDestroy,
    Migrate { from: u32, to: u32 },
    SelectiveDisclosure,
    FieldInitializer(GlobalId),
    Test,
}

#[derive(Debug, Clone)]
pub struct IrParam {
    pub name: Ident,
    pub ty: Ty,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub enum IrGuard {
    When(Value),
    Only(IrPrincipal),
    Given(Value),
}

#[derive(Debug, Clone)]
pub enum IrPrincipal {
    Caller,
    Deployer,
    Owner(Option<GlobalId>),
    Admin(Option<GlobalId>),
    Parties(Option<GlobalId>),
    Guardians(Option<GlobalId>),
    Holders,
    BuiltinPredicate(BuiltinPredicate, Vec<Value>),
    Address(Value),
    Unresolved,
}

#[derive(Debug, Clone)]
pub enum IrActionQualifier {
    VerifiedBy(Value),
    PqSigned { msg: Value, sig: Value, pk: Value },
    VdfLocked(Value),
}

#[derive(Debug, Clone)]
pub enum IrAnnotation {
    Precompute(Value),
    BatchUpTo(u32),
    ProveOffchain,
    GasBudget { l1: Option<u64>, pgas: Option<u64> },
    Unknown(Box<str>),
}
