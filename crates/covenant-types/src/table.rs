//! Type table: per-expression types, per-local types, per-decl types, and
//! nominal-type registries (structs / credentials / choices).

use std::collections::HashMap;

use covenant_diag::Span;
use covenant_parser::ast::Ident;
use covenant_resolver::{DeclId, LocalId};

use crate::ty::{ChoiceTypeId, CredentialId, StructId, Ty};

#[derive(Debug, Clone, Default)]
pub struct TypeTable {
    /// Expression span → its type.
    pub expr_types: HashMap<Span, Ty>,

    /// `LocalId.0` → the local binding's type.
    pub local_types: Vec<Ty>,

    /// `DeclId.0` → the declaration's type (for fields) or callable shape (for
    /// actions/views/reveals) or struct/credential ID wrapping.
    pub decl_types: Vec<Ty>,

    /// All interned structs. `StructId.0` indexes here.
    pub structs: Vec<StructInfo>,

    /// All interned credentials. `CredentialId.0` indexes here.
    pub credentials: Vec<CredentialInfo>,

    /// All interned choice types. `ChoiceTypeId.0` indexes here.
    pub choices: Vec<ChoiceTypeInfo>,

    /// Error declarations: `DeclId` → positional arg types.
    pub error_signatures: HashMap<DeclId, Vec<Ty>>,

    /// Event declarations: `DeclId` → positional arg types.
    pub event_signatures: HashMap<DeclId, Vec<Ty>>,

    /// Lift markers: locations where a plaintext value was auto-lifted into a
    /// ciphertext context. Phase 6 (IR) uses these to insert FHE trivial-
    /// encryption ops.
    pub lifts: Vec<LiftMarker>,
}

impl TypeTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern_struct(&mut self, info: StructInfo) -> StructId {
        let id = StructId(self.structs.len() as u32);
        self.structs.push(info);
        id
    }

    pub fn intern_credential(&mut self, info: CredentialInfo) -> CredentialId {
        let id = CredentialId(self.credentials.len() as u32);
        self.credentials.push(info);
        id
    }

    pub fn intern_choice(&mut self, info: ChoiceTypeInfo) -> ChoiceTypeId {
        let id = ChoiceTypeId(self.choices.len() as u32);
        self.choices.push(info);
        id
    }

    pub fn record_expr(&mut self, span: Span, ty: Ty) {
        self.expr_types.insert(span, ty);
    }

    pub fn get_expr(&self, span: &Span) -> Option<&Ty> {
        self.expr_types.get(span)
    }

    pub fn struct_info(&self, id: StructId) -> Option<&StructInfo> {
        self.structs.get(id.0 as usize)
    }

    pub fn credential_info(&self, id: CredentialId) -> Option<&CredentialInfo> {
        self.credentials.get(id.0 as usize)
    }

    pub fn choice_info(&self, id: ChoiceTypeId) -> Option<&ChoiceTypeInfo> {
        self.choices.get(id.0 as usize)
    }

    pub fn set_local(&mut self, id: LocalId, ty: Ty) {
        let i = id.0 as usize;
        if self.local_types.len() <= i {
            self.local_types.resize(i + 1, Ty::Unknown);
        }
        self.local_types[i] = ty;
    }

    pub fn local_ty(&self, id: LocalId) -> Ty {
        self.local_types
            .get(id.0 as usize)
            .cloned()
            .unwrap_or(Ty::Unknown)
    }

    pub fn set_decl(&mut self, id: DeclId, ty: Ty) {
        let i = id.0 as usize;
        if self.decl_types.len() <= i {
            self.decl_types.resize(i + 1, Ty::Unknown);
        }
        self.decl_types[i] = ty;
    }

    pub fn decl_ty(&self, id: DeclId) -> Ty {
        self.decl_types
            .get(id.0 as usize)
            .cloned()
            .unwrap_or(Ty::Unknown)
    }

    pub fn record_lift(&mut self, lift: LiftMarker) {
        self.lifts.push(lift);
    }
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: Ident,
    pub fields: Vec<StructField>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: Ident,
    pub ty: Ty,
    pub indexed: bool,
}

#[derive(Debug, Clone)]
pub struct CredentialInfo {
    pub name: Ident,
    pub fields: Vec<StructField>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ChoiceTypeInfo {
    pub members: Vec<Box<str>>,
    pub is_inline: bool,
    pub span: Span,
    /// For named choice types (future: `choice my_side = [...]`). V0 has inline
    /// only, so this stays `None`.
    pub name: Option<Box<str>>,
}

/// Auto-lift record: a plaintext expression auto-encrypted to ciphertext.
#[derive(Debug, Clone)]
pub struct LiftMarker {
    pub span: Span,
    pub from: Ty,
    pub to: Ty,
}
