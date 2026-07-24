//! IR builder: lowers a `PrivacyCheckedFile` into an `IrModule`.
//!
//! The builder walks each construct body, creating one `IrFunction` per
//! behavior (action / view / reveal / on_destroy / migrate / selective_disclosure)
//! plus synthetic field-initializer functions for non-trivial initializers.
//! Statements and expressions are lowered into SSA blocks with block
//! parameters. Auto-lift markers from Phase 4 are materialized as explicit
//! `FheEncryptTrivial` instructions.

use std::collections::{HashMap, HashSet};

use covenant_diag::{Diagnostic, Span};
use covenant_parser::ast::{
    ActionDecl, ActionQualifier, AnchorClause, AnnotationArg, AssignOp, BinaryOp, ConstructKind,
    Expr, Ident, LValue, LiteralExpr, MetadataValue, OnDestroyBlock, Principal, PrincipalKind,
    PrivacyQualifier, RevealDecl, RevealTarget, Stmt, TopLevelDecl, UnaryOp, UpgradeableClause,
    ViewDecl,
};
use covenant_privacy::{PrivacyCheckedFile, PrivacyDomain};
use covenant_resolver::{Binding, BuiltinPredicate, LangIdent, LocalId, StdlibFn, StdlibModule};
use covenant_types::Ty;

use crate::block::IrBlock;
use crate::diag;
use crate::function::{
    IrActionQualifier, IrAnnotation, IrFunction, IrFunctionKind, IrGuard, IrParam, IrPrincipal,
};
use crate::id::{BlockId, FunctionId, GlobalId, StructTypeId, Value};
use crate::instr::{Instr, InstrMetadata, IrConstant, StdlibCall, Terminator, ValueInfo};
use crate::module::{
    AnchorInfo, IrChoice, IrError, IrEvent, IrExternalContract, IrExternalFunc, IrField,
    IrMetadataValue, IrModule, IrStruct, IrStructField, UpgradeableInfo,
};
use crate::opcode::Opcode;

/// `func_name → (abi_sig, is_view, param_count)` for one external contract.
type ExtFuncMap = HashMap<Box<str>, (Box<str>, bool, u32)>;

/// A `list<Struct>` field's element struct: covenant-ir's own `StructTypeId`
/// (for `Opcode::StructNew`) plus its fields' `(name, type)` in DECLARED
/// order (append literals may list fields in a different order).
type StructAppendMeta = (StructTypeId, Vec<(Box<str>, Ty)>);

pub struct Builder {
    checked: PrivacyCheckedFile,
    diagnostics: Vec<Diagnostic>,

    // Module-level state
    fields: Vec<IrField>,
    structs: Vec<IrStruct>,
    errors: Vec<IrError>,
    events: Vec<IrEvent>,
    choices: Vec<IrChoice>,
    functions: Vec<IrFunction>,
    external_contracts: Vec<IrExternalContract>,
    metadata: HashMap<Box<str>, IrMetadataValue>,
    anchor: Option<AnchorInfo>,
    upgradeable: Option<UpgradeableInfo>,

    // Name → field id lookup (for lowering identifier references to SLoad).
    field_by_name: HashMap<Box<str>, (GlobalId, Ty, PrivacyDomain)>,

    // Struct name → struct id lookup.
    struct_by_name: HashMap<Box<str>, StructTypeId>,

    // External contract lookup: contract_name → func_name → (abi_sig, is_view, param_count).
    ext_by_name: HashMap<Box<str>, ExtFuncMap>,

    // Event / error name sets for validation.
    event_names: HashSet<Box<str>>,
    error_names: HashSet<Box<str>>,

    // Lift marker spans (from Phase 4) for quick lookup during expression lowering.
    lift_spans: HashSet<Span>,
}

impl Builder {
    pub fn new(checked: PrivacyCheckedFile) -> Self {
        Self {
            checked,
            diagnostics: Vec::new(),
            fields: Vec::new(),
            structs: Vec::new(),
            errors: Vec::new(),
            events: Vec::new(),
            choices: Vec::new(),
            functions: Vec::new(),
            external_contracts: Vec::new(),
            metadata: HashMap::new(),
            anchor: None,
            upgradeable: None,
            field_by_name: HashMap::new(),
            struct_by_name: HashMap::new(),
            ext_by_name: HashMap::new(),
            event_names: HashSet::new(),
            error_names: HashSet::new(),
            lift_spans: HashSet::new(),
        }
    }

    pub fn run(mut self) -> (IrModule, Vec<Diagnostic>) {
        // Build lift-span lookup once.
        for lift in &self.checked.typed.types.lifts {
            self.lift_spans.insert(lift.span);
        }

        // Module-level: anchor and upgradeable clauses.
        let tl = self.checked.typed.resolved.file.top_level.clone();
        if let Some(a) = &tl.anchor {
            self.anchor = Some(lower_anchor(a));
        }
        if let Some(up) = &tl.upgradeable {
            self.upgradeable = Some(lower_upgradeable(up));
        }

        // Pass A: register nominal types (structs, credentials as structs,
        // errors, events, choices) and metadata; register field IDs.
        self.pass_register(&tl.body);

        // Lower external contract declarations (builds lookup table for call lowering).
        let ext_decls = self.checked.typed.resolved.file.external_contracts.clone();
        self.lower_external_contracts(&ext_decls);

        // Pass B: lower each declaration body into IR.
        let body = tl.body.clone();
        for d in &body {
            self.lower_top_level_decl(d);
        }

        let module = IrModule {
            source_id: self.checked.typed.resolved.file.source_id,
            name: tl.name.clone(),
            construct_kind: tl.keyword,
            construct_privacy: tl.privacy,
            fields: self.fields,
            structs: self.structs,
            errors: self.errors,
            events: self.events,
            choices: self.choices,
            functions: self.functions,
            external_contracts: self.external_contracts,
            metadata: self.metadata,
            anchor: self.anchor,
            upgradeable: self.upgradeable,
        };
        (module, self.diagnostics)
    }

    // -----------------------------------------------------------------
    // Pass A: register globals
    // -----------------------------------------------------------------

    fn pass_register(&mut self, body: &[TopLevelDecl]) {
        let mut next_field_id = 0u32;
        let mut next_struct_id = 0u32;

        // Register choices from the TypeTable first so struct/field types that
        // reference Choice(id) can be rendered later.
        for c in &self.checked.typed.types.choices {
            self.choices.push(IrChoice {
                members: c.members.clone(),
                is_inline: c.is_inline,
                span: c.span,
            });
        }

        for d in body {
            match d {
                TopLevelDecl::Field(f) => {
                    let field_id = GlobalId(next_field_id);
                    next_field_id += 1;
                    let base_ty = self.lower_ast_type(&f.ty);
                    let effective_ty = self.apply_privacy(&f.per_field_privacy, base_ty);
                    let privacy = covenant_privacy::domain_of(&effective_ty);
                    let explicit_slot =
                        extract_slot_annotation(&f.annotations, &mut self.diagnostics);
                    self.field_by_name.insert(
                        f.name.name.clone(),
                        (field_id, effective_ty.clone(), privacy),
                    );
                    self.fields.push(IrField {
                        id: field_id,
                        name: f.name.clone(),
                        ty: effective_ty,
                        privacy,
                        initializer_fn: None,
                        // Carry a constant field default (`n: amount = 42`) into
                        // the IR so the constructor SSTOREs it. This was
                        // dropped unconditionally, so a non-zero default read
                        // back as 0 on chain (verified 2026-07-23). Only the
                        // literal types the backend's `emit_const_initializer`
                        // stores in one word are carried; see
                        // `field_default_const`.
                        initializer_const: f.initializer.as_ref().and_then(field_default_const),
                        span: f.span,
                        explicit_slot,
                    });
                }
                TopLevelDecl::Struct(s) => {
                    let id = StructTypeId(next_struct_id);
                    next_struct_id += 1;
                    let fields = s
                        .fields
                        .iter()
                        .map(|sf| IrStructField {
                            name: sf.name.clone(),
                            ty: self.lower_ast_type(&sf.ty),
                            indexed: false,
                        })
                        .collect();
                    self.struct_by_name.insert(s.name.name.clone(), id);
                    self.structs.push(IrStruct {
                        id,
                        name: s.name.clone(),
                        fields,
                        span: s.span,
                    });
                }
                TopLevelDecl::Credential(c) => {
                    // Treat credentials as structs at IR level for V0.
                    let id = StructTypeId(next_struct_id);
                    next_struct_id += 1;
                    let fields = c
                        .fields
                        .iter()
                        .map(|sf| IrStructField {
                            name: sf.name.clone(),
                            ty: self.lower_ast_type(&sf.ty),
                            indexed: false,
                        })
                        .collect();
                    self.struct_by_name.insert(c.name.name.clone(), id);
                    self.structs.push(IrStruct {
                        id,
                        name: c.name.clone(),
                        fields,
                        span: c.span,
                    });
                }
                TopLevelDecl::Error(e) => {
                    self.error_names.insert(e.name.name.clone());
                    let params = e
                        .fields
                        .iter()
                        .map(|f| self.lower_ast_type(&f.ty))
                        .collect();
                    self.errors.push(IrError {
                        name: e.name.clone(),
                        params,
                        span: e.span,
                    });
                }
                TopLevelDecl::Event(e) => {
                    self.event_names.insert(e.name.name.clone());
                    let params = e
                        .args
                        .iter()
                        .map(|a| (a.name.clone(), self.lower_ast_type(&a.ty), a.indexed))
                        .collect();
                    self.events.push(IrEvent {
                        name: e.name.clone(),
                        params,
                        span: e.span,
                    });
                }
                TopLevelDecl::Metadata(m) => {
                    let v = self.lower_metadata_value(&m.value);
                    self.metadata.insert(m.name.name.clone(), v);
                }
                _ => {}
            }
        }
    }

    #[allow(clippy::only_used_in_recursion)]
    fn lower_metadata_value(&self, mv: &MetadataValue) -> IrMetadataValue {
        match mv {
            MetadataValue::Literal(e) => match e {
                Expr::Literal(LiteralExpr::Integer(n, _)) => IrMetadataValue::Integer(*n),
                Expr::Literal(LiteralExpr::Text(s, _)) => IrMetadataValue::Text(s.clone()),
                Expr::Literal(LiteralExpr::Bool(b, _)) => IrMetadataValue::Bool(*b),
                Expr::Literal(LiteralExpr::Duration(n, u, _)) => {
                    IrMetadataValue::Duration { n: *n, unit: *u }
                }
                Expr::Literal(LiteralExpr::Hex(bytes, _)) => IrMetadataValue::Hex(bytes.clone()),
                Expr::Array { elements, .. } => {
                    let items = elements
                        .iter()
                        .map(|el| self.lower_metadata_value(&MetadataValue::Literal(el.clone())))
                        .collect();
                    IrMetadataValue::Array(items)
                }
                _ => IrMetadataValue::Bool(false),
            },
            MetadataValue::GenesisMint { amount, to } => {
                let amt = match amount {
                    Expr::Literal(LiteralExpr::Integer(n, _)) => *n,
                    _ => 0,
                };
                IrMetadataValue::GenesisMint {
                    amount: amt,
                    to: to.clone(),
                }
            }
        }
    }

    fn apply_privacy(&self, per: &Option<PrivacyQualifier>, base: Ty) -> Ty {
        match per {
            Some(PrivacyQualifier::Public) => base,
            Some(PrivacyQualifier::Encrypted) => Ty::Ciphertext(Box::new(base)),
            _ => {
                let construct_enc = matches!(
                    self.checked.typed.resolved.file.top_level.privacy,
                    Some(PrivacyQualifier::Encrypted)
                        | Some(PrivacyQualifier::Sealed)
                        | Some(PrivacyQualifier::Confidential)
                );
                if construct_enc {
                    Ty::Ciphertext(Box::new(base))
                } else {
                    base
                }
            }
        }
    }

    fn lower_ast_type(&self, t: &covenant_parser::ast::Type) -> Ty {
        lower_ast_type_impl(t)
    }

    // -----------------------------------------------------------------
    // Pass B: lower each decl body into IR
    // -----------------------------------------------------------------

    fn lower_top_level_decl(&mut self, d: &TopLevelDecl) {
        match d {
            TopLevelDecl::Action(a) => {
                let f = self.lower_action(a);
                self.functions.push(f);
            }
            TopLevelDecl::View(v) => {
                let f = self.lower_view(v);
                self.functions.push(f);
            }
            TopLevelDecl::Reveal(r) => {
                let f = self.lower_reveal(r);
                self.functions.push(f);
            }
            TopLevelDecl::OnDestroy(od) => {
                let f = self.lower_on_destroy(od);
                self.functions.push(f);
            }
            TopLevelDecl::SelectiveDisclosure(sd) => {
                // Deferred: emit a diagnostic and skip.
                self.diagnostics
                    .push(diag::selective_disclosure_deferred(sd.span));
            }
            _ => {}
        }
    }

    fn next_function_id(&self) -> FunctionId {
        FunctionId(self.functions.len() as u32)
    }

    fn lower_action(&mut self, a: &ActionDecl) -> IrFunction {
        let func_id = self.next_function_id();
        // A `test_*` action (or one carrying `@test`) is a test, not part of
        // the contract's public interface. Classify it as `Test` so
        // `is_public_function` strips it from the ABI, the selector table and
        // the runtime dispatcher on a release build. Before this, every test
        // shipped as a public entrypoint — and a test that mutates is a public
        // UNGUARDED state mutator (DEBT.md; confirmed on-chain 2026-07-23).
        let kind = if is_test_action(a) {
            IrFunctionKind::Test
        } else {
            IrFunctionKind::Action
        };
        let mut fb = FunctionBuilder::new(func_id, a.name.clone(), kind, a.span);

        // Parameters.
        for (idx, arg) in a.args.iter().enumerate() {
            let ty = self.lower_ast_type(&arg.ty);
            let v = fb.fresh_value(ValueInfo::Param {
                func: func_id,
                index: idx as u32,
            });
            fb.value_types.insert(v, ty.clone());
            fb.value_privacy.insert(v, covenant_privacy::domain_of(&ty));
            fb.value_spans.insert(v, arg.name.span);
            fb.params.push(IrParam {
                name: arg.name.clone(),
                ty,
                value: v,
            });
            if let Some(lid) = self.find_arg_local(&arg.name) {
                fb.local_to_value.insert(lid, v);
            }
        }

        // Qualifiers → pre-body verify sequence.
        for q in &a.qualifiers {
            let lowered = self.lower_qualifier(&mut fb, q);
            fb.qualifiers.push(lowered);
        }

        // Guards → pre-body Assert sequence.
        for g in &a.guards {
            let lowered = self.lower_guard(&mut fb, g);
            fb.guards.push(lowered);
        }

        // Annotations (metadata only).
        for ann in &a.annotations {
            validate_annotation_name(ann, &mut self.diagnostics);
            fb.annotations.push(lower_annotation(ann));
        }

        // Body statements.
        for s in &a.body {
            self.lower_stmt(&mut fb, s);
        }

        // Default terminator: Return(None) if none was set.
        fb.close_with_default_return(None);
        fb.finish()
    }

    fn lower_view(&mut self, v: &ViewDecl) -> IrFunction {
        let func_id = self.next_function_id();
        let mut fb = FunctionBuilder::new(func_id, v.name.clone(), IrFunctionKind::View, v.span);
        fb.returns = v.returns.as_ref().map(|t| self.lower_ast_type(t));

        for (idx, arg) in v.args.iter().enumerate() {
            let ty = self.lower_ast_type(&arg.ty);
            let val = fb.fresh_value(ValueInfo::Param {
                func: func_id,
                index: idx as u32,
            });
            fb.value_types.insert(val, ty.clone());
            fb.value_privacy
                .insert(val, covenant_privacy::domain_of(&ty));
            fb.value_spans.insert(val, arg.name.span);
            fb.params.push(IrParam {
                name: arg.name.clone(),
                ty,
                value: val,
            });
            if let Some(lid) = self.find_arg_local(&arg.name) {
                fb.local_to_value.insert(lid, val);
            }
        }

        for g in &v.guards {
            let lowered = self.lower_guard(&mut fb, g);
            fb.guards.push(lowered);
        }

        let body_value = self.lower_expr(&mut fb, &v.body);
        fb.current_block_mut().terminator = Terminator::Return(Some(body_value));
        fb.finish()
    }

    fn lower_reveal(&mut self, r: &RevealDecl) -> IrFunction {
        let func_id = self.next_function_id();
        let mut fb = FunctionBuilder::new(func_id, r.name.clone(), IrFunctionKind::Reveal, r.span);
        fb.returns = r.returns.as_ref().map(|t| self.lower_ast_type(t));

        for (idx, arg) in r.args.iter().enumerate() {
            let ty = self.lower_ast_type(&arg.ty);
            let val = fb.fresh_value(ValueInfo::Param {
                func: func_id,
                index: idx as u32,
            });
            fb.value_types.insert(val, ty.clone());
            fb.value_privacy
                .insert(val, covenant_privacy::domain_of(&ty));
            fb.params.push(IrParam {
                name: arg.name.clone(),
                ty,
                value: val,
            });
            if let Some(lid) = self.find_arg_local(&arg.name) {
                fb.local_to_value.insert(lid, val);
            }
        }

        for g in &r.guards {
            let lowered = self.lower_guard(&mut fb, g);
            fb.guards.push(lowered);
        }

        // F07 (CRITICAL): enforce the `to <target>` disclosure restriction.
        // The one-liner `reveal <field> to owner` used to DROP the target
        // entirely — the reveal compiled with zero caller check, so the
        // owner-only restriction was silently unenforced (private_dao.cov even
        // documented "Access control (`to owner`) is IR-level metadata; EVM
        // enforcement is V1"). Lower the target into the SAME authorization
        // assertion machinery that `only <principal>` guards use, so a
        // non-owner call reverts before anything is disclosed. This runs before
        // the body/return below, so the check gates the disclosure.
        if let Some(target) = &r.target {
            let principal = self.reveal_target_principal(&mut fb, target);
            self.emit_only_assert(&mut fb, &principal, r.span);
        }

        match &r.body {
            Some(body) => {
                let v = self.lower_expr(&mut fb, body);
                // Wrap with RevealDecrypt if expression type is still Ciphertext.
                let v_ty = fb.value_types.get(&v).cloned().unwrap_or(Ty::Unknown);
                let v_out = if matches!(v_ty, Ty::Ciphertext(_)) {
                    let inner_ty = v_ty.strip_ciphertext().cloned().unwrap_or(Ty::Unknown);
                    fb.emit_instr(
                        Opcode::RevealDecrypt,
                        vec![v],
                        Some(inner_ty.clone()),
                        PrivacyDomain::Plaintext,
                        body.span(),
                        InstrMetadata::default(),
                    )
                } else {
                    v
                };
                fb.current_block_mut().terminator = Terminator::Return(Some(v_out));
            }
            None => {
                // One-liner: `reveal <field> to <target>`.
                let (field_id, field_ty, _dom) = match self.field_by_name.get(r.name.name.as_ref())
                {
                    Some(info) => info.clone(),
                    None => {
                        fb.current_block_mut().terminator = Terminator::Return(None);
                        return fb.finish();
                    }
                };
                let loaded = fb.emit_instr(
                    Opcode::SLoad(field_id),
                    vec![],
                    Some(field_ty.clone()),
                    covenant_privacy::domain_of(&field_ty),
                    r.span,
                    InstrMetadata::default(),
                );
                let v_out = if matches!(field_ty, Ty::Ciphertext(_)) {
                    let inner_ty = field_ty.strip_ciphertext().cloned().unwrap_or(Ty::Unknown);
                    fb.emit_instr(
                        Opcode::RevealDecrypt,
                        vec![loaded],
                        Some(inner_ty.clone()),
                        PrivacyDomain::Plaintext,
                        r.span,
                        InstrMetadata::default(),
                    )
                } else {
                    loaded
                };
                fb.current_block_mut().terminator = Terminator::Return(Some(v_out));
            }
        }
        fb.finish()
    }

    /// F07: map a `reveal ... to <target>` restriction onto the same
    /// `IrPrincipal` vocabulary that `only <principal>` guards use, so
    /// `emit_only_assert` can lower it to a real caller check.
    fn reveal_target_principal(
        &mut self,
        fb: &mut FunctionBuilder,
        t: &RevealTarget,
    ) -> IrPrincipal {
        match t {
            // `to owner`: gate on the `owner` field if the construct declares
            // one, otherwise on the deployer — the natural on-chain owner of a
            // construct with no explicit owner field (e.g. an `encrypted
            // counter`). This reuses `only owner` / `only deployer` codegen
            // verbatim and matches the harness's own expectation that the
            // reveal recipient "owner = deployer".
            RevealTarget::Owner => match self.field_by_name.get("owner") {
                Some((gid, _, _)) => IrPrincipal::Owner(Some(*gid)),
                None => IrPrincipal::Deployer,
            },
            // `to caller`: the caller IS the recipient, so `caller == caller`
            // is trivially true — an unrestricted (public) reveal. No check.
            RevealTarget::Caller => IrPrincipal::Caller,
            // `to parties`: a collection-membership check that is not yet
            // lowered — `emit_only_assert` fails it CLOSED (reverts every call)
            // with the KSR-CVN-011 diagnostic, never silently open.
            RevealTarget::Parties => {
                IrPrincipal::Parties(self.field_by_name.get("parties").map(|(id, _, _)| *id))
            }
            // `to address(expr)`: gate on the evaluated address.
            RevealTarget::Address(e) => {
                let v = self.lower_expr(fb, e);
                IrPrincipal::Address(v)
            }
        }
    }

    fn lower_on_destroy(&mut self, od: &OnDestroyBlock) -> IrFunction {
        let func_id = self.next_function_id();
        let synth_name = Ident {
            name: "on_destroy".into(),
            span: od.span,
        };
        let mut fb = FunctionBuilder::new(func_id, synth_name, IrFunctionKind::OnDestroy, od.span);
        for s in &od.body {
            self.lower_stmt(&mut fb, s);
        }
        fb.close_with_default_return(None);
        fb.finish()
    }

    fn find_arg_local(&self, ident: &Ident) -> Option<LocalId> {
        self.checked
            .typed
            .resolved
            .bindings
            .locals
            .iter()
            .find(|l| l.def_span == ident.span)
            .map(|l| l.id)
    }

    /// The type-checker's OWN, authoritative type for a field identifier
    /// (via its `DeclId`) -- NOT `field_by_name`'s locally-cached `Ty`, which
    /// is computed by this crate's own `lower_ast_type_impl` and always
    /// collapses any `Type::User(_)` (a named `struct`/`credential` type) to
    /// `Ty::Unknown` (it has no name->id registry to resolve against). For
    /// anything that needs to see THROUGH a `List<Struct(..)>`/`Struct(..)`
    /// field type (OMEGA V6 CRT-003/MED-001 follow-ons), use this instead.
    fn real_field_ty(&self, ident: &Ident) -> Option<Ty> {
        match self.checked.typed.resolved.bindings.get(&ident.span) {
            Some(Binding::Field(decl_id)) => Some(self.checked.typed.types.decl_ty(*decl_id)),
            _ => None,
        }
    }

    fn lower_qualifier(
        &mut self,
        fb: &mut FunctionBuilder,
        q: &ActionQualifier,
    ) -> IrActionQualifier {
        match q {
            ActionQualifier::VerifiedBy(e) => {
                let v = self.lower_expr(fb, e);
                // Emit ZkVerify + Assert pre-body.
                let public_inputs = fb.emit_const(
                    IrConstant::Hex(Box::from([] as [u8; 0])),
                    Ty::Bytes,
                    e.span(),
                );
                let circuit_id =
                    fb.emit_const(IrConstant::Hash(Box::new([0u8; 32])), Ty::Hash, e.span());
                let ok = fb.emit_instr(
                    Opcode::ZkVerify,
                    vec![circuit_id, v, public_inputs],
                    Some(Ty::Bool),
                    PrivacyDomain::Plaintext,
                    e.span(),
                    InstrMetadata::default(),
                );
                fb.emit_instr(
                    Opcode::Assert,
                    vec![ok],
                    None,
                    PrivacyDomain::Plaintext,
                    e.span(),
                    InstrMetadata::default(),
                );
                IrActionQualifier::VerifiedBy(v)
            }
            ActionQualifier::PqSigned { msg, sig, pk, .. } => {
                let msg_v = self.lower_expr(fb, msg);
                let sig_v = self.lower_expr(fb, sig);
                let pk_v = self.lower_expr(fb, pk);
                // Hash the message, then verify.
                let hashed = fb.emit_instr(
                    Opcode::Keccak,
                    vec![msg_v],
                    Some(Ty::Hash),
                    PrivacyDomain::Plaintext,
                    msg.span(),
                    InstrMetadata::default(),
                );
                let ok = fb.emit_instr(
                    Opcode::PqVerifyDilithium,
                    vec![pk_v, hashed, sig_v],
                    Some(Ty::Bool),
                    PrivacyDomain::Plaintext,
                    msg.span(),
                    InstrMetadata::default(),
                );
                fb.emit_instr(
                    Opcode::Assert,
                    vec![ok],
                    None,
                    PrivacyDomain::Plaintext,
                    msg.span(),
                    InstrMetadata::default(),
                );
                IrActionQualifier::PqSigned {
                    msg: msg_v,
                    sig: sig_v,
                    pk: pk_v,
                }
            }
            ActionQualifier::VdfLocked { delay, .. } => {
                let v = self.lower_expr(fb, delay);
                IrActionQualifier::VdfLocked(v)
            }
        }
    }

    fn lower_guard(
        &mut self,
        fb: &mut FunctionBuilder,
        g: &covenant_parser::ast::Guard,
    ) -> IrGuard {
        use covenant_parser::ast::Guard;
        match g {
            Guard::When(e) => {
                let v = self.lower_expr(fb, e);
                let op = if matches!(
                    fb.value_types.get(&v).cloned().unwrap_or(Ty::Unknown),
                    Ty::Ciphertext(_)
                ) {
                    Opcode::AssertEncrypted
                } else {
                    Opcode::Assert
                };
                fb.emit_instr(
                    op,
                    vec![v],
                    None,
                    PrivacyDomain::Plaintext,
                    e.span(),
                    InstrMetadata::default(),
                );
                IrGuard::When(v)
            }
            Guard::Given(e) => {
                let v = self.lower_expr(fb, e);
                fb.emit_instr(
                    Opcode::Assert,
                    vec![v],
                    None,
                    PrivacyDomain::Plaintext,
                    e.span(),
                    InstrMetadata::default(),
                );
                IrGuard::Given(v)
            }
            Guard::Only(p) => {
                let lowered = self.lower_principal(fb, p);
                let span = Span::new(self.checked.typed.resolved.file.source_id, 0, 0);
                // KSR-CVN-011: lower every principal-based `only` clause into a
                // real IR assertion. Prior to this, non-predicate principals
                // (owner / admin / deployer / address(...)) fell through to
                // `Assert(true)` — a no-op that made access control advisory
                // across every contract produced by the compiler.
                self.emit_only_assert(fb, &lowered, span);
                IrGuard::Only(lowered)
            }
        }
    }

    /// KSR-CVN-011: emit an authorization assertion for an `only(principal)`
    /// guard. Each branch loads the appropriate address (or boolean), compares
    /// it to the caller, and asserts the result. Unsupported / unresolved
    /// principals fail closed via `Assert(false)` so the function reverts on
    /// every call rather than being silently open.
    fn emit_only_assert(&mut self, fb: &mut FunctionBuilder, p: &IrPrincipal, span: Span) {
        let bool_ty = Ty::Bool;
        let addr_ty = Ty::Address;
        let plain = PrivacyDomain::Plaintext;
        let meta = InstrMetadata::default();

        let assert_eq_caller = |fb: &mut FunctionBuilder, principal_v: Value| {
            let caller = fb.emit_instr(
                Opcode::LoadCaller,
                vec![],
                Some(addr_ty.clone()),
                plain,
                span,
                meta.clone(),
            );
            let eq = fb.emit_instr(
                Opcode::Eq,
                vec![caller, principal_v],
                Some(bool_ty.clone()),
                plain,
                span,
                meta.clone(),
            );
            fb.emit_instr(Opcode::Assert, vec![eq], None, plain, span, meta.clone());
        };

        let assert_false = |fb: &mut FunctionBuilder| {
            let f = fb.emit_const(IrConstant::Bool(false), bool_ty.clone(), span);
            fb.emit_instr(Opcode::Assert, vec![f], None, plain, span, meta.clone());
        };

        match p {
            // `only caller` — trivially true (caller == caller). No emit.
            IrPrincipal::Caller => {}

            // `only deployer` — assert caller == constructor-captured deployer.
            IrPrincipal::Deployer => {
                let dep = fb.emit_instr(
                    Opcode::LoadDeployer,
                    vec![],
                    Some(addr_ty.clone()),
                    plain,
                    span,
                    meta.clone(),
                );
                assert_eq_caller(fb, dep);
            }

            // `only owner` / `only admin` — assert caller == SLOAD(field_slot).
            IrPrincipal::Owner(Some(gid)) | IrPrincipal::Admin(Some(gid)) => {
                let principal_v = fb.emit_instr(
                    Opcode::SLoad(*gid),
                    vec![],
                    Some(addr_ty.clone()),
                    plain,
                    span,
                    meta.clone(),
                );
                assert_eq_caller(fb, principal_v);
            }

            // `only address(expr)` — assert caller == expr.
            IrPrincipal::Address(v) => {
                assert_eq_caller(fb, *v);
            }

            // `only first_time_caller`, `only registered_key`, etc.
            IrPrincipal::BuiltinPredicate(bp, args) => {
                let ok = fb.emit_instr(
                    Opcode::BuiltinPredicateCall(*bp),
                    args.clone(),
                    Some(bool_ty.clone()),
                    plain,
                    span,
                    meta.clone(),
                );
                fb.emit_instr(Opcode::Assert, vec![ok], None, plain, span, meta.clone());
            }

            // Unresolved field bindings (no `owner`/`admin` field declared) and
            // collection-typed principals (parties / guardians / holders) are
            // not yet lowered to runtime bytecode. Fail closed so the function
            // reverts on every call rather than silently allowing access.
            IrPrincipal::Owner(None)
            | IrPrincipal::Admin(None)
            | IrPrincipal::Parties(_)
            | IrPrincipal::Guardians(_)
            | IrPrincipal::Holders
            | IrPrincipal::Unresolved => {
                self.diagnostics.push(diag::guard_unresolved(span, p));
                assert_false(fb);
            }
        }
    }

    fn lower_principal(&mut self, fb: &mut FunctionBuilder, p: &Principal) -> IrPrincipal {
        match p {
            Principal::Named(PrincipalKind::Caller) => IrPrincipal::Caller,
            Principal::Named(PrincipalKind::Deployer) => IrPrincipal::Deployer,
            Principal::Named(PrincipalKind::Owner) => {
                IrPrincipal::Owner(self.field_by_name.get("owner").map(|(id, _, _)| *id))
            }
            Principal::Named(PrincipalKind::Admin) => {
                IrPrincipal::Admin(self.field_by_name.get("admin").map(|(id, _, _)| *id))
            }
            Principal::Named(PrincipalKind::Parties) => {
                IrPrincipal::Parties(self.field_by_name.get("parties").map(|(id, _, _)| *id))
            }
            Principal::Named(PrincipalKind::Guardians) => {
                IrPrincipal::Guardians(self.field_by_name.get("guardians").map(|(id, _, _)| *id))
            }
            Principal::Named(PrincipalKind::Holders) => IrPrincipal::Holders,
            Principal::Predicate(ident) => match BuiltinPredicate::from_name(&ident.name) {
                Some(bp) => IrPrincipal::BuiltinPredicate(bp, vec![]),
                None => IrPrincipal::Unresolved,
            },
            Principal::Address(e) => {
                let v = self.lower_expr(fb, e);
                IrPrincipal::Address(v)
            }
            Principal::Call { name, args } => {
                let bp = BuiltinPredicate::from_name(&name.name)
                    .unwrap_or(BuiltinPredicate::ValidatorMajority);
                let arg_vals = args.iter().map(|e| self.lower_expr(fb, e)).collect();
                IrPrincipal::BuiltinPredicate(bp, arg_vals)
            }
        }
    }

    // -----------------------------------------------------------------
    // Statement lowering
    // -----------------------------------------------------------------

    fn lower_stmt(&mut self, fb: &mut FunctionBuilder, s: &Stmt) {
        match s {
            Stmt::Let { name, value, .. } => {
                let v = self.lower_expr(fb, value);
                if let Some(lid) = self.find_arg_local(name) {
                    fb.local_to_value.insert(lid, v);
                }
            }
            Stmt::Assign {
                target, op, value, ..
            } => {
                let rhs = self.lower_expr(fb, value);
                self.lower_assign(fb, target, *op, rhs, value.span());
            }
            Stmt::Expr(e, _) => {
                self.lower_expr(fb, e);
            }
            Stmt::If {
                cond,
                then_block,
                else_ifs,
                else_block,
                span,
            } => {
                let cond_v = self.lower_expr(fb, cond);
                let then_id = fb.new_block(*span);
                let else_id = fb.new_block(*span);
                let merge_id = fb.new_block(*span);
                fb.current_block_mut().terminator = Terminator::Branch {
                    cond: cond_v,
                    then_target: then_id,
                    then_args: vec![],
                    else_target: else_id,
                    else_args: vec![],
                };

                fb.set_current(then_id);
                for s in then_block {
                    self.lower_stmt(fb, s);
                }
                // NOTE (KSR-CVN, OMEGA V6 CRT-002 fix): `new_block()` initializes every
                // fresh block's terminator to `Terminator::Unreachable` as an internal
                // "not yet lowered" placeholder (see `new_block` / `close_with_default_return`
                // below -- that is the ONLY place this crate ever assigns `Unreachable`,
                // there is no surface-language construct that produces it intentionally).
                // The old check here also treated `Unreachable` as "already terminated",
                // which meant an empty/absent branch (never touched, still carrying the
                // placeholder) was silently treated as a deliberate trap instead of falling
                // through to `merge_id` -- orphaning every statement after the `if` behind
                // a permanently disconnected block that the optimizer's DCE pass would then
                // delete as genuinely unreachable. Only a real terminal statement
                // (`Return`/`Revert`) should suppress the fallthrough jump.
                if !matches!(
                    fb.current_block().terminator,
                    Terminator::Return(_) | Terminator::Revert { .. }
                ) {
                    fb.current_block_mut().terminator = Terminator::Jump {
                        target: merge_id,
                        args: vec![],
                    };
                }

                fb.set_current(else_id);
                // else_ifs chained; produce a linear structure.
                for (c, b) in else_ifs {
                    let c_v = self.lower_expr(fb, c);
                    let ei_then = fb.new_block(*span);
                    let ei_else = fb.new_block(*span);
                    fb.current_block_mut().terminator = Terminator::Branch {
                        cond: c_v,
                        then_target: ei_then,
                        then_args: vec![],
                        else_target: ei_else,
                        else_args: vec![],
                    };
                    fb.set_current(ei_then);
                    for s in b {
                        self.lower_stmt(fb, s);
                    }
                    if !matches!(
                        fb.current_block().terminator,
                        Terminator::Return(_) | Terminator::Revert { .. }
                    ) {
                        fb.current_block_mut().terminator = Terminator::Jump {
                            target: merge_id,
                            args: vec![],
                        };
                    }
                    fb.set_current(ei_else);
                }
                if let Some(b) = else_block {
                    for s in b {
                        self.lower_stmt(fb, s);
                    }
                }
                // See the CRT-002 note above the `then_id` closing check: an absent or
                // empty `else` must fall through to `merge_id`, not be mistaken for a
                // deliberate trap just because it still carries the `new_block()` placeholder.
                if !matches!(
                    fb.current_block().terminator,
                    Terminator::Return(_) | Terminator::Revert { .. }
                ) {
                    fb.current_block_mut().terminator = Terminator::Jump {
                        target: merge_id,
                        args: vec![],
                    };
                }
                fb.set_current(merge_id);
            }
            Stmt::EncryptedWhen {
                cond,
                then_block,
                otherwise,
                span,
            } => {
                let cond_v = self.lower_expr(fb, cond);
                let then_id = fb.new_block(*span);
                let else_id = fb.new_block(*span);
                let merge_id = fb.new_block(*span);
                fb.current_block_mut().terminator = Terminator::FheBranch {
                    cond: cond_v,
                    then_target: then_id,
                    then_args: vec![],
                    else_target: else_id,
                    else_args: vec![],
                    merge_target: merge_id,
                };
                fb.set_current(then_id);
                for s in then_block {
                    self.lower_stmt(fb, s);
                }
                fb.current_block_mut().terminator = Terminator::Jump {
                    target: merge_id,
                    args: vec![],
                };
                fb.set_current(else_id);
                if let Some(o) = otherwise {
                    for s in o {
                        self.lower_stmt(fb, s);
                    }
                }
                fb.current_block_mut().terminator = Terminator::Jump {
                    target: merge_id,
                    args: vec![],
                };
                fb.set_current(merge_id);
            }
            Stmt::Match { span, .. } => {
                // V0 simplification: match lowering reduced to a no-op with a
                // placeholder choice-match opcode. Full lowering is deferred
                // until the optimizer gets structural pattern semantics.
                let _ = span;
            }
            Stmt::ForEach {
                binding,
                iter,
                body,
                span,
            } => {
                // OMEGA V6 CRT-003 fix: real structured-loop lowering with a
                // counter threaded through a block parameter (the same
                // mechanism already used for if-expression merge values,
                // see `Expr::If` above) — header block branches on
                // `counter < len`, body block reads `ListGet(list, counter)`
                // into the loop binding and jumps back to the header with
                // `counter + 1`. The previous lowering ran the body exactly
                // once with no back-edge and never bound the loop variable
                // to a real element at all.
                let list_v = self.lower_expr(fb, iter);
                let list_ty = fb.value_types.get(&list_v).cloned().unwrap_or(Ty::Unknown);
                let elem_ty = match &list_ty {
                    Ty::List(inner) => (**inner).clone(),
                    _ => Ty::Unknown,
                };
                let elem_dom = covenant_privacy::domain_of(&elem_ty);

                let len_v = fb.emit_instr(
                    Opcode::ListLength,
                    vec![list_v],
                    Some(Ty::Amount),
                    PrivacyDomain::Plaintext,
                    *span,
                    InstrMetadata::default(),
                );

                let header_id = fb.new_block(*span);
                let body_id = fb.new_block(*span);
                let merge_id = fb.new_block(*span);

                let counter_param = fb.fresh_value(ValueInfo::BlockParam {
                    block: header_id,
                    index: 0,
                });
                fb.value_types.insert(counter_param, Ty::Amount);
                fb.value_privacy
                    .insert(counter_param, PrivacyDomain::Plaintext);
                fb.block_mut(header_id).params.push(counter_param);

                let zero_v = fb.emit_const(IrConstant::Integer(0), Ty::Amount, *span);
                fb.current_block_mut().terminator = Terminator::Jump {
                    target: header_id,
                    args: vec![zero_v],
                };

                fb.set_current(header_id);
                let cond_v = fb.emit_instr(
                    Opcode::Lt,
                    vec![counter_param, len_v],
                    Some(Ty::Bool),
                    PrivacyDomain::Plaintext,
                    *span,
                    InstrMetadata::default(),
                );
                fb.current_block_mut().terminator = Terminator::Branch {
                    cond: cond_v,
                    then_target: body_id,
                    then_args: vec![],
                    else_target: merge_id,
                    else_args: vec![],
                };

                fb.set_current(body_id);
                let elem_v = fb.emit_instr(
                    Opcode::ListGet,
                    vec![list_v, counter_param],
                    Some(elem_ty),
                    elem_dom,
                    *span,
                    InstrMetadata::default(),
                );
                if let Some(lid) = self.find_arg_local(binding) {
                    fb.local_to_value.insert(lid, elem_v);
                }
                for s in body {
                    self.lower_stmt(fb, s);
                }
                // A `return`/`revert` inside the loop body legitimately exits the
                // function early; only add the back-edge when the body fell
                // through normally (same discipline as the CRT-002 If fix).
                if !matches!(
                    fb.current_block().terminator,
                    Terminator::Return(_) | Terminator::Revert { .. }
                ) {
                    let one_v = fb.emit_const(IrConstant::Integer(1), Ty::Amount, *span);
                    let next_counter = fb.emit_instr(
                        Opcode::AddChecked,
                        vec![counter_param, one_v],
                        Some(Ty::Amount),
                        PrivacyDomain::Plaintext,
                        *span,
                        InstrMetadata::default(),
                    );
                    fb.current_block_mut().terminator = Terminator::Jump {
                        target: header_id,
                        args: vec![next_counter],
                    };
                }

                fb.set_current(merge_id);
            }
            Stmt::TryCatch {
                body,
                catch_body,
                span,
                ..
            } => {
                for s in body {
                    self.lower_stmt(fb, s);
                }
                let _ = span;
                let _ = catch_body;
            }
            Stmt::Emit { event, args, span } => {
                let arg_vs: Vec<Value> = args.iter().map(|a| self.lower_expr(fb, a)).collect();
                fb.emit_instr(
                    Opcode::Emit(Box::new(event.clone())),
                    arg_vs,
                    None,
                    PrivacyDomain::Plaintext,
                    *span,
                    InstrMetadata::default(),
                );
            }
            Stmt::Transfer {
                amount,
                from,
                to,
                span,
            } => {
                let amt_v = self.lower_expr(fb, amount);
                let to_v = self.lower_expr(fb, to);
                let mut ops = vec![amt_v];
                if let Some(f) = from {
                    let f_v = self.lower_expr(fb, f);
                    ops.push(f_v);
                }
                ops.push(to_v);
                fb.emit_instr(
                    Opcode::Transfer,
                    ops,
                    None,
                    PrivacyDomain::Plaintext,
                    *span,
                    InstrMetadata::default(),
                );
            }
            Stmt::Append {
                collection,
                struct_lit,
                span,
            } => {
                // OMEGA V6 (CRT-003 follow-on): `append` used to build a struct
                // value via StructNew and then discard it (`let _ = struct_val`)
                // -- nothing was ever written to storage, so a list populated
                // only via `append` was always empty and CRT-003's for-each fix
                // would have had nothing real to iterate over. This now
                // actually persists the element.
                //
                // Resolve the struct type from the COLLECTION FIELD's declared
                // element type (`field_by_name[collection]` -> `List<Struct(sid)>`)
                // -- the old code looked up `struct_by_name[collection.name]`,
                // which is the *list field's* name (e.g. "votes"), not the
                // *struct type's* name (e.g. "SecretVote"), so it almost always
                // missed and silently fell back to `StructTypeId(0)`.
                let list_field = self.field_by_name.get(collection.name.as_ref()).cloned();
                // Use the type-checker's OWN type for `collection` (via
                // `real_field_ty`), NOT `field_by_name`'s locally-cached `Ty`
                // -- this crate's own `lower_ast_type_impl` collapses every
                // `Type::User(_)` (a named struct type) to `Ty::Unknown`, so
                // `field_by_name`'s stored type for a `list<SecretVote>`
                // field is `List<Unknown>`, never `List<Struct(sid)>`.
                let real_list_ty = self.real_field_ty(collection);
                // Extract everything needed as OWNED data up front: `struct_info`
                // borrows `self.checked` for the lifetime of the `&StructInfo`,
                // which would otherwise conflict with the `&mut self` needed by
                // `self.lower_expr(..)` below.
                let struct_meta: Option<StructAppendMeta> =
                    real_list_ty.as_ref().and_then(|ty| match ty {
                        Ty::List(inner) => match inner.as_ref() {
                            Ty::Struct(sid) => {
                                self.checked.typed.types.struct_info(*sid).map(|info| {
                                    let sid = self
                                        .struct_by_name
                                        .get(info.name.name.as_ref())
                                        .cloned()
                                        .unwrap_or(StructTypeId(0));
                                    let field_order = info
                                        .fields
                                        .iter()
                                        .map(|f| (f.name.name.clone(), f.ty.clone()))
                                        .collect();
                                    (sid, field_order)
                                })
                            }
                            _ => None,
                        },
                        _ => None,
                    });
                let struct_id = struct_meta
                    .as_ref()
                    .map(|(sid, _)| *sid)
                    .unwrap_or(StructTypeId(0));

                // Order the literal's field values by the struct's DECLARED
                // field order (append literals may list fields in any order),
                // so `StructGet(k)`/`StructSet(k)` (which index by declared
                // position) read/write the field the source actually named.
                let operands: Vec<Value> = if let Some((_, field_order)) = &struct_meta {
                    field_order
                        .iter()
                        .map(|(decl_name, decl_ty)| {
                            let found = struct_lit.iter().find(|fa| &fa.name.name == decl_name);
                            match found {
                                Some(fa) => self.lower_expr(fb, &fa.value),
                                None => {
                                    fb.emit_const(IrConstant::Integer(0), decl_ty.clone(), *span)
                                }
                            }
                        })
                        .collect()
                } else {
                    struct_lit
                        .iter()
                        .map(|fa| self.lower_expr(fb, &fa.value))
                        .collect()
                };

                let struct_val = fb.emit_instr(
                    Opcode::StructNew(struct_id),
                    operands,
                    Some(Ty::Unknown),
                    PrivacyDomain::Plaintext,
                    *span,
                    InstrMetadata::default(),
                );

                if let Some((list_field_id, _list_ty, _)) = list_field {
                    // Tag this SLoad with the REAL list type (`List<Struct(sid)>`),
                    // not a placeholder -- codegen's `ListAppend` needs to see the
                    // element type here (via `f.value_types`) to compute the
                    // per-element storage stride.
                    let list_base = fb.emit_instr(
                        Opcode::SLoad(list_field_id),
                        vec![],
                        Some(real_list_ty.clone().unwrap_or(Ty::Unknown)),
                        PrivacyDomain::Plaintext,
                        *span,
                        InstrMetadata::default(),
                    );
                    let new_len = fb.emit_instr(
                        Opcode::ListAppend,
                        vec![list_base, struct_val],
                        Some(Ty::Amount),
                        PrivacyDomain::Plaintext,
                        *span,
                        InstrMetadata::default(),
                    );
                    fb.emit_instr(
                        Opcode::SStore(list_field_id),
                        vec![new_len],
                        None,
                        PrivacyDomain::Plaintext,
                        *span,
                        InstrMetadata::default(),
                    );
                }
                // If `collection` doesn't resolve to a registered field, the
                // resolver has already rejected the program (unresolved
                // identifier) before IR construction is reached -- nothing
                // further to do here.
            }
            Stmt::Discard(_, _) | Stmt::Delete(_, _) => {
                // Discard is a no-op at IR level; Delete is handled similarly
                // until map-aware deletion lands.
            }
            Stmt::Return(e, _) => {
                let v_opt = e.as_ref().map(|expr| self.lower_expr(fb, expr));
                fb.current_block_mut().terminator = Terminator::Return(v_opt);
            }
            Stmt::RevertWith { error, args, .. } => {
                let arg_vs = args.iter().map(|a| self.lower_expr(fb, a)).collect();
                fb.current_block_mut().terminator = Terminator::Revert {
                    error: error.clone(),
                    args: arg_vs,
                };
            }
        }
    }

    fn lower_assign(
        &mut self,
        fb: &mut FunctionBuilder,
        target: &LValue,
        op: AssignOp,
        rhs: Value,
        span: Span,
    ) {
        // Walk to an l-value kind: Ident (field or local), FieldAccess, or Index.
        match target {
            LValue::Ident(id) => {
                if let Some((field_id, field_ty, _dom)) =
                    self.field_by_name.get(id.name.as_ref()).cloned()
                {
                    let value_to_store = if matches!(op, AssignOp::Eq) {
                        rhs
                    } else {
                        let cur = fb.emit_instr(
                            Opcode::SLoad(field_id),
                            vec![],
                            Some(field_ty.clone()),
                            covenant_privacy::domain_of(&field_ty),
                            id.span,
                            InstrMetadata::default(),
                        );
                        let binop = match op {
                            AssignOp::PlusEq => Opcode::AddChecked,
                            AssignOp::MinusEq => Opcode::SubChecked,
                            AssignOp::StarEq => Opcode::MulChecked,
                            AssignOp::SlashEq => Opcode::Div,
                            AssignOp::PercentEq => Opcode::Mod,
                            AssignOp::Eq => unreachable!(),
                        };
                        // If field is Ciphertext and rhs is Plaintext, lift rhs.
                        let rhs_ty = fb.value_types.get(&rhs).cloned().unwrap_or(Ty::Unknown);
                        let (lifted_rhs, lifted_op) = if matches!(field_ty, Ty::Ciphertext(_))
                            && !matches!(rhs_ty, Ty::Ciphertext(_))
                            && !matches!(rhs_ty, Ty::Unknown)
                        {
                            let lifted = fb.emit_instr(
                                Opcode::FheEncryptTrivial,
                                vec![rhs],
                                Some(field_ty.clone()),
                                PrivacyDomain::Encrypted,
                                span,
                                InstrMetadata {
                                    from_lift: true,
                                    ..Default::default()
                                },
                            );
                            let fhe_op = match op {
                                AssignOp::PlusEq => Opcode::FheAdd,
                                AssignOp::MinusEq => Opcode::FheSub,
                                AssignOp::StarEq => Opcode::FheMul,
                                _ => binop,
                            };
                            (lifted, fhe_op)
                        } else if matches!(field_ty, Ty::Ciphertext(_)) {
                            let fhe_op = match op {
                                AssignOp::PlusEq => Opcode::FheAdd,
                                AssignOp::MinusEq => Opcode::FheSub,
                                AssignOp::StarEq => Opcode::FheMul,
                                _ => binop,
                            };
                            (rhs, fhe_op)
                        } else {
                            (rhs, binop)
                        };
                        fb.emit_instr(
                            lifted_op,
                            vec![cur, lifted_rhs],
                            Some(field_ty.clone()),
                            covenant_privacy::domain_of(&field_ty),
                            span,
                            InstrMetadata::default(),
                        )
                    };
                    fb.emit_instr(
                        Opcode::SStore(field_id),
                        vec![value_to_store],
                        None,
                        PrivacyDomain::Plaintext,
                        span,
                        InstrMetadata::default(),
                    );
                    return;
                }
                // Assignment to a local: update the `local_to_value` mapping.
                if let Some(lid) = self.find_arg_local(id) {
                    fb.local_to_value.insert(lid, rhs);
                }
            }
            LValue::Index(base, key_expr) => {
                if let LValue::Ident(base_id) = base.as_ref() {
                    if let Some((field_id, _field_ty, _)) =
                        self.field_by_name.get(base_id.name.as_ref()).cloned()
                    {
                        let key = self.lower_expr(fb, key_expr);
                        let cur = fb.emit_instr(
                            Opcode::SLoad(field_id),
                            vec![],
                            Some(Ty::Unknown),
                            PrivacyDomain::Plaintext,
                            base_id.span,
                            InstrMetadata::default(),
                        );
                        // For compound operators, read-modify-write the map slot.
                        let value_to_store = if matches!(op, AssignOp::Eq) {
                            rhs
                        } else {
                            let old = fb.emit_instr(
                                Opcode::MapGet,
                                vec![cur, key],
                                Some(Ty::Unknown),
                                PrivacyDomain::Plaintext,
                                base_id.span,
                                InstrMetadata::default(),
                            );
                            let binop = match op {
                                AssignOp::PlusEq => Opcode::AddChecked,
                                AssignOp::MinusEq => Opcode::SubChecked,
                                AssignOp::StarEq => Opcode::MulChecked,
                                AssignOp::SlashEq => Opcode::Div,
                                AssignOp::PercentEq => Opcode::Mod,
                                AssignOp::Eq => unreachable!(),
                            };
                            fb.emit_instr(
                                binop,
                                vec![old, rhs],
                                Some(Ty::Unknown),
                                PrivacyDomain::Plaintext,
                                span,
                                InstrMetadata::default(),
                            )
                        };
                        let updated = fb.emit_instr(
                            Opcode::MapSet,
                            vec![cur, key, value_to_store],
                            Some(Ty::Unknown),
                            PrivacyDomain::Plaintext,
                            span,
                            InstrMetadata::default(),
                        );
                        fb.emit_instr(
                            Opcode::SStore(field_id),
                            vec![updated],
                            None,
                            PrivacyDomain::Plaintext,
                            span,
                            InstrMetadata::default(),
                        );
                    }
                }
            }
            LValue::FieldAccess(base, field) => {
                // OMEGA V6 (MED-001 follow-on): this used to be a hard no-op --
                // `list[idx].field = value` silently dropped the write entirely,
                // even setting aside the privacy analyzer's separate false-positive
                // rejection of the same pattern. Handles the reachable, supported
                // shape: a struct-typed element of a `list<Struct>` field, addressed
                // via `ListGet` and written through `StructSet(field_idx)`.
                if let LValue::Index(list_base, idx_expr) = base.as_ref() {
                    if let LValue::Ident(list_ident) = list_base.as_ref() {
                        let list_field_id_opt = self
                            .field_by_name
                            .get(list_ident.name.as_ref())
                            .map(|(id, _, _)| *id);
                        // See the matching note in Stmt::Append: use the type-checker's
                        // real type, not field_by_name's Ty::Unknown-collapsed one.
                        let real_list_ty = self.real_field_ty(list_ident);
                        if let (Some(list_field_id), Some(list_ty)) =
                            (list_field_id_opt, real_list_ty)
                        {
                            if let Ty::List(inner) = &list_ty {
                                if let Ty::Struct(sid) = inner.as_ref() {
                                    let field_info: Option<(u32, Ty)> =
                                        self.checked.typed.types.struct_info(*sid).and_then(
                                            |info| {
                                                info.fields
                                                    .iter()
                                                    .position(|f| f.name.name == field.name)
                                                    .map(|idx| {
                                                        (idx as u32, info.fields[idx].ty.clone())
                                                    })
                                            },
                                        );
                                    if let Some((field_idx, field_ty)) = field_info {
                                        let idx_v = self.lower_expr(fb, idx_expr);
                                        // Tag with the real List<Struct> type -- see the
                                        // matching comment in Stmt::Append.
                                        let list_v = fb.emit_instr(
                                            Opcode::SLoad(list_field_id),
                                            vec![],
                                            Some(list_ty.clone()),
                                            PrivacyDomain::Plaintext,
                                            span,
                                            InstrMetadata::default(),
                                        );
                                        let addr = fb.emit_instr(
                                            Opcode::ListGet,
                                            vec![list_v, idx_v],
                                            Some(Ty::Struct(*sid)),
                                            PrivacyDomain::Plaintext,
                                            span,
                                            InstrMetadata::default(),
                                        );
                                        let value_to_store = if matches!(op, AssignOp::Eq) {
                                            rhs
                                        } else {
                                            let old = fb.emit_instr(
                                                Opcode::StructGet(field_idx),
                                                vec![addr],
                                                Some(field_ty.clone()),
                                                covenant_privacy::domain_of(&field_ty),
                                                span,
                                                InstrMetadata::default(),
                                            );
                                            let binop = match op {
                                                AssignOp::PlusEq => Opcode::AddChecked,
                                                AssignOp::MinusEq => Opcode::SubChecked,
                                                AssignOp::StarEq => Opcode::MulChecked,
                                                AssignOp::SlashEq => Opcode::Div,
                                                AssignOp::PercentEq => Opcode::Mod,
                                                AssignOp::Eq => unreachable!(),
                                            };
                                            fb.emit_instr(
                                                binop,
                                                vec![old, rhs],
                                                Some(field_ty.clone()),
                                                covenant_privacy::domain_of(&field_ty),
                                                span,
                                                InstrMetadata::default(),
                                            )
                                        };
                                        fb.emit_instr(
                                            Opcode::StructSet(field_idx),
                                            vec![addr, value_to_store],
                                            None,
                                            PrivacyDomain::Plaintext,
                                            span,
                                            InstrMetadata::default(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                // Fallback: other FieldAccess shapes (e.g. a bare struct-typed
                // top-level field not held in a list) are not yet supported --
                // this needs multi-slot storage allocation for such fields,
                // tracked as a follow-up, not silently mis-compiled here.
            }
        }
    }

    // -----------------------------------------------------------------
    // Expression lowering
    // -----------------------------------------------------------------

    fn lower_expr(&mut self, fb: &mut FunctionBuilder, e: &Expr) -> Value {
        let result = self.lower_expr_inner(fb, e);
        // Apply auto-lift if Phase 4 flagged this span.
        if self.lift_spans.contains(&e.span()) {
            // Only lift if the resulting value is plaintext.
            let is_plain = matches!(
                fb.value_types.get(&result).cloned().unwrap_or(Ty::Unknown),
                Ty::Unknown
            ) || !matches!(
                fb.value_types.get(&result).cloned().unwrap_or(Ty::Unknown),
                Ty::Ciphertext(_)
            );
            if is_plain {
                let src_ty = fb.value_types.get(&result).cloned().unwrap_or(Ty::Unknown);
                return fb.emit_instr(
                    Opcode::FheEncryptTrivial,
                    vec![result],
                    Some(Ty::Ciphertext(Box::new(src_ty))),
                    PrivacyDomain::Encrypted,
                    e.span(),
                    InstrMetadata {
                        from_lift: true,
                        ..Default::default()
                    },
                );
            }
        }
        result
    }

    fn lower_expr_inner(&mut self, fb: &mut FunctionBuilder, e: &Expr) -> Value {
        let ty = self
            .checked
            .typed
            .types
            .expr_types
            .get(&e.span())
            .cloned()
            .unwrap_or(Ty::Unknown);
        let dom = covenant_privacy::domain_of(&ty);
        match e {
            Expr::Literal(lit) => self.lower_literal(fb, lit, ty),
            Expr::Ident(id) => self.lower_ident(fb, id, ty),
            Expr::Binary { op, lhs, rhs, span } => {
                // Fail-loud, mirroring the E424 stdlib-math refusal. The `in`
                // membership operator has no real lowering: `choose_binop` maps
                // exactly one opcode, but membership is a `ListContains` loop
                // (compare + branch over each element). The old placeholder
                // returned `Opcode::Eq`, so `x in [a, b, c]` compiled to
                // `x == a` — a guard that silently passed only for the first
                // element and rejected every other member, with no diagnostic.
                if matches!(op, BinaryOp::In) {
                    // Still lower operands so their spans/types are recorded,
                    // then refuse — matching the emit-const-after-diag shape of
                    // E424/E425.
                    let _ = self.lower_expr(fb, lhs);
                    let _ = self.lower_expr(fb, rhs);
                    self.diagnostics
                        .push(diag::membership_in_unimplemented(*span));
                    return fb.emit_const(IrConstant::Integer(0), ty, *span);
                }
                let l = self.lower_expr(fb, lhs);
                let r = self.lower_expr(fb, rhs);
                let l_ty = fb.value_types.get(&l).cloned().unwrap_or(Ty::Unknown);
                let r_ty = fb.value_types.get(&r).cloned().unwrap_or(Ty::Unknown);
                let opcode = choose_binop(*op, &l_ty, &r_ty);
                fb.emit_instr(
                    opcode,
                    vec![l, r],
                    Some(ty),
                    dom,
                    *span,
                    InstrMetadata::default(),
                )
            }
            Expr::Unary { op, operand, span } => {
                let v = self.lower_expr(fb, operand);
                let opcode = match op {
                    UnaryOp::Not => Opcode::LogicalNot,
                    UnaryOp::Neg => Opcode::SignedNeg,
                    UnaryOp::BitNot => Opcode::BitNot,
                };
                fb.emit_instr(
                    opcode,
                    vec![v],
                    Some(ty),
                    dom,
                    *span,
                    InstrMetadata::default(),
                )
            }
            Expr::Call {
                callee, args, span, ..
            } => self.lower_call(fb, callee, args, *span, ty, dom),
            Expr::FieldAccess { base, field, span } => {
                let base_v = self.lower_expr(fb, base);
                let base_ty = fb.value_types.get(&base_v).cloned().unwrap_or(Ty::Unknown);
                let opcode = match &base_ty {
                    Ty::Namespace(covenant_types::Namespace::Msg) => match field.name.as_ref() {
                        "sender" => Opcode::LoadMsgSender,
                        "value" => Opcode::LoadMsgValue,
                        _ => Opcode::StructGet(0),
                    },
                    Ty::Namespace(covenant_types::Namespace::Block) => match field.name.as_ref() {
                        "timestamp" => Opcode::LoadBlockTimestamp,
                        "number" => Opcode::LoadBlockNumber,
                        _ => Opcode::StructGet(0),
                    },
                    Ty::List(_) => match field.name.as_ref() {
                        "length" | "len" => Opcode::ListLength,
                        "first" => Opcode::ListFirst,
                        "last" => Opcode::ListLast,
                        "argmax" => Opcode::ListArgMax,
                        "argmin" => Opcode::ListArgMin,
                        _ => Opcode::StructGet(0),
                    },
                    // Fail-loud, mirroring the E424 stdlib-math refusal above.
                    // These lowered to opcodes the EVM backend answered with
                    // PUSH0, so `.length` was always 0 and `.keys`/`.values`
                    // yielded a handle that `for each` read as an empty list —
                    // a clean-compiling silent no-op, indistinguishable from a
                    // genuinely empty map. A Covenant map is a bare
                    // keccak(key ‖ slot) mapping: it has no length word and no
                    // key array, so there is nothing correct to emit here.
                    Ty::Map(_, _) => match field.name.as_ref() {
                        member @ ("length" | "len" | "keys" | "values") => {
                            self.diagnostics
                                .push(diag::map_introspection_unimplemented(*span, member));
                            return fb.emit_const(IrConstant::Integer(0), ty, *span);
                        }
                        // Fail-loud, mirroring the E425 map-introspection
                        // refusal. `.argmax`/`.argmin` fell through to
                        // `StructGet(0)`: the reduction never iterated and
                        // returned a constant (field 0 of the map handle)
                        // instead of the key with the max/min value — a
                        // clean-compiling silent miscompile. A Covenant map has
                        // no key array to iterate, so nothing correct can be
                        // emitted. List `.argmax`/`.argmin` still lower via the
                        // `Ty::List` arm above (ListArgMax / ListArgMin).
                        member @ ("argmax" | "argmin") => {
                            self.diagnostics
                                .push(diag::map_arg_reduction_unimplemented(*span, member));
                            return fb.emit_const(IrConstant::Integer(0), ty, *span);
                        }
                        _ => Opcode::StructGet(0),
                    },
                    // OMEGA V6 (MED-001 follow-on): resolve the REAL declared
                    // field index instead of hardcoding 0 for every struct
                    // field read. `StructGet(k)`/`StructSet(k)` index by
                    // declared position, so any field other than the first
                    // was previously read as if it were the first.
                    Ty::Struct(sid) => {
                        let idx = self
                            .checked
                            .typed
                            .types
                            .struct_info(*sid)
                            .and_then(|info| {
                                info.fields.iter().position(|f| f.name.name == field.name)
                            })
                            .unwrap_or(0) as u32;
                        Opcode::StructGet(idx)
                    }
                    _ => Opcode::StructGet(0),
                };
                let operands = if matches!(
                    opcode,
                    Opcode::LoadMsgSender
                        | Opcode::LoadMsgValue
                        | Opcode::LoadBlockTimestamp
                        | Opcode::LoadBlockNumber
                ) {
                    vec![]
                } else {
                    vec![base_v]
                };
                fb.emit_instr(
                    opcode,
                    operands,
                    Some(ty),
                    dom,
                    *span,
                    InstrMetadata::default(),
                )
            }
            Expr::Index {
                base, index, span, ..
            } => {
                let base_v = self.lower_expr(fb, base);
                let idx_v = self.lower_expr(fb, index);
                let base_ty = fb.value_types.get(&base_v).cloned().unwrap_or(Ty::Unknown);
                let opcode = match base_ty {
                    Ty::Map(_, _) => Opcode::MapGet,
                    Ty::List(_) => Opcode::ListGet,
                    _ => Opcode::MapGet, // default
                };
                fb.emit_instr(
                    opcode,
                    vec![base_v, idx_v],
                    Some(ty),
                    dom,
                    *span,
                    InstrMetadata::default(),
                )
            }
            Expr::Slice {
                base,
                from,
                to,
                span,
            } => {
                let b = self.lower_expr(fb, base);
                let f = self.lower_expr(fb, from);
                let t = self.lower_expr(fb, to);
                fb.emit_instr(
                    Opcode::ListSlice,
                    vec![b, f, t],
                    Some(ty),
                    dom,
                    *span,
                    InstrMetadata::default(),
                )
            }
            Expr::Array { elements, span } => {
                // Synthesize a list via repeated ListAppend on a zero list.
                // For V0 we emit a single-instruction placeholder with all
                // elements as operands of StructNew (any list-builder opcode
                // would need a dedicated opcode; we use StructNew(0) as a
                // stand-in). A future pass should promote this to a proper
                // ListBuild opcode.
                let operands: Vec<Value> =
                    elements.iter().map(|el| self.lower_expr(fb, el)).collect();
                fb.emit_instr(
                    Opcode::StructNew(StructTypeId(0)),
                    operands,
                    Some(ty),
                    dom,
                    *span,
                    InstrMetadata::default(),
                )
            }
            Expr::Paren(inner) => self.lower_expr(fb, inner),
            Expr::If {
                cond,
                then_expr,
                else_expr,
                span,
            } => {
                // For expressions, create blocks and a merge block with a
                // block parameter for the value.
                let cond_v = self.lower_expr(fb, cond);
                let then_id = fb.new_block(*span);
                let else_id = fb.new_block(*span);
                let merge_id = fb.new_block(*span);
                // Merge block has one parameter carrying the expression result.
                let merge_param = fb.fresh_value(ValueInfo::BlockParam {
                    block: merge_id,
                    index: 0,
                });
                fb.value_types.insert(merge_param, ty.clone());
                fb.value_privacy.insert(merge_param, dom);
                fb.block_mut(merge_id).params.push(merge_param);

                fb.current_block_mut().terminator = Terminator::Branch {
                    cond: cond_v,
                    then_target: then_id,
                    then_args: vec![],
                    else_target: else_id,
                    else_args: vec![],
                };
                fb.set_current(then_id);
                let t_v = self.lower_expr(fb, then_expr);
                fb.current_block_mut().terminator = Terminator::Jump {
                    target: merge_id,
                    args: vec![t_v],
                };
                fb.set_current(else_id);
                let e_v = self.lower_expr(fb, else_expr);
                fb.current_block_mut().terminator = Terminator::Jump {
                    target: merge_id,
                    args: vec![e_v],
                };
                fb.set_current(merge_id);
                merge_param
            }
            Expr::Match {
                scrutinee, span, ..
            } => {
                let _ = self.lower_expr(fb, scrutinee);
                // Placeholder ChoiceMatch; full lowering deferred.
                fb.emit_const(IrConstant::Integer(0), ty, *span)
            }
            Expr::EncryptedLit(inner, span) => {
                let v = self.lower_expr(fb, inner);
                fb.emit_instr(
                    Opcode::FheEncryptTrivial,
                    vec![v],
                    Some(ty),
                    PrivacyDomain::Encrypted,
                    *span,
                    InstrMetadata::default(),
                )
            }
            Expr::Lambda { span, .. } => {
                self.diagnostics.push(diag::lambda_unsupported(*span));
                fb.emit_const(IrConstant::Integer(0), ty, *span)
            }
        }
    }

    fn lower_literal(&mut self, fb: &mut FunctionBuilder, lit: &LiteralExpr, ty: Ty) -> Value {
        match lit {
            LiteralExpr::Integer(n, span) => fb.emit_const(IrConstant::Integer(*n), ty, *span),
            LiteralExpr::Text(s, span) => fb.emit_const(IrConstant::Text(s.clone()), ty, *span),
            LiteralExpr::Bool(b, span) => fb.emit_const(IrConstant::Bool(*b), ty, *span),
            LiteralExpr::Duration(n, u, span) => {
                fb.emit_const(IrConstant::Duration { n: *n, unit: *u }, ty, *span)
            }
            LiteralExpr::Hex(bytes, span) => match bytes.len() {
                20 => {
                    let mut arr = [0u8; 20];
                    arr.copy_from_slice(bytes);
                    fb.emit_const(IrConstant::Address(Box::new(arr)), ty, *span)
                }
                32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(bytes);
                    fb.emit_const(IrConstant::Hash(Box::new(arr)), ty, *span)
                }
                _ => fb.emit_const(IrConstant::Hex(bytes.clone()), ty, *span),
            },
        }
    }

    fn lower_ident(&mut self, fb: &mut FunctionBuilder, id: &Ident, ty: Ty) -> Value {
        match self.checked.typed.resolved.bindings.get(&id.span).cloned() {
            Some(Binding::Field(decl_id)) => {
                let info = self.field_by_name.get(id.name.as_ref()).cloned();
                if let Some((fid, fty, dom)) = info {
                    // OMEGA V6 (CRT-003 follow-on): `fty` is this crate's own
                    // `lower_ast_type_impl` output, which collapses any
                    // `Type::User(_)` (a named struct/credential type) to
                    // `Ty::Unknown` -- so a `list<SecretVote>` field reports
                    // as `List<Unknown>` here, which broke `Stmt::ForEach`'s
                    // element-type extraction (`Ty::List(inner) => *inner`)
                    // for any list-of-struct field referenced by plain
                    // identifier. Prefer the type-checker's real, struct-aware
                    // type in exactly that (List<Unknown>/Map<_,Unknown>/bare
                    // Unknown) case; otherwise keep the existing cached type
                    // unchanged (it already correctly reflects privacy
                    // qualifiers like `@encrypted` for every other field kind).
                    let needs_precise_ty = matches!(&fty, Ty::List(inner) if matches!(inner.as_ref(), Ty::Unknown))
                        || matches!(&fty, Ty::Map(_, v) if matches!(v.as_ref(), Ty::Unknown))
                        || matches!(fty, Ty::Unknown);
                    let resolved_ty = if needs_precise_ty {
                        self.checked.typed.types.decl_ty(decl_id)
                    } else {
                        fty
                    };
                    fb.emit_instr(
                        Opcode::SLoad(fid),
                        vec![],
                        Some(resolved_ty),
                        dom,
                        id.span,
                        InstrMetadata::default(),
                    )
                } else {
                    fb.emit_const(IrConstant::Integer(0), ty, id.span)
                }
            }
            Some(Binding::Local(lid)) => {
                let val = fb.local_to_value.get(&lid).copied();
                val.unwrap_or_else(|| fb.emit_const(IrConstant::Integer(0), ty, id.span))
            }
            Some(Binding::LangIdent(li)) => self.lower_lang_ident(fb, li, ty, id.span),
            Some(Binding::StdlibFn(_)) | Some(Binding::StdlibModule(_)) => {
                // Appears as a callee target — the Call branch handles dispatch.
                fb.emit_const(IrConstant::Integer(0), ty, id.span)
            }
            _ => fb.emit_const(IrConstant::Integer(0), ty, id.span),
        }
    }

    fn lower_lang_ident(
        &mut self,
        fb: &mut FunctionBuilder,
        li: LangIdent,
        ty: Ty,
        span: Span,
    ) -> Value {
        let opcode = match li {
            LangIdent::Caller => Opcode::LoadCaller,
            LangIdent::Now | LangIdent::OpensAt | LangIdent::OpenedAt | LangIdent::ClosesAt => {
                Opcode::LoadNow
            }
            LangIdent::CurrentBlock => Opcode::LoadBlockNumber,
            LangIdent::Deployer => Opcode::LoadDeployer,
            LangIdent::This => Opcode::LoadThis,
            LangIdent::ZeroAddress => Opcode::LoadZeroAddress,
            LangIdent::Block | LangIdent::Msg => {
                // Namespaces — represent as a placeholder value; subsequent
                // FieldAccess dispatches to the concrete opcode.
                return fb.emit_const(IrConstant::Integer(0), ty, span);
            }
            _ => return fb.emit_const(IrConstant::Integer(0), ty, span),
        };
        fb.emit_instr(
            opcode,
            vec![],
            Some(ty),
            PrivacyDomain::Plaintext,
            span,
            InstrMetadata::default(),
        )
    }

    fn lower_call(
        &mut self,
        fb: &mut FunctionBuilder,
        callee: &Expr,
        args: &[Expr],
        span: Span,
        result_ty: Ty,
        dom: PrivacyDomain,
    ) -> Value {
        // Stdlib free function dispatch.
        if let Expr::Ident(id) = callee {
            if let Some(Binding::StdlibFn(f)) =
                self.checked.typed.resolved.bindings.get(&id.span).cloned()
            {
                // These have no lowering. They used to fall through to
                // `AddChecked`, so `max(cap, bid)` compiled to `cap + bid`
                // and shipped. Refuse instead of guessing.
                if let Some(name) = unimplemented_math_name(f) {
                    self.diagnostics
                        .push(diag::stdlib_math_unimplemented(span, name));
                    return fb.emit_const(IrConstant::Integer(0), result_ty, span);
                }
                let opcode = stdlib_fn_opcode(f);
                let arg_vs: Vec<Value> = args.iter().map(|a| self.lower_expr(fb, a)).collect();
                return fb.emit_instr(
                    opcode,
                    arg_vs,
                    Some(result_ty),
                    dom,
                    span,
                    InstrMetadata {
                        stdlib_call: Some(StdlibCall::Free(f)),
                        ..Default::default()
                    },
                );
            }
        }
        // Stdlib module method dispatch.
        if let Expr::FieldAccess { base, field, .. } = callee {
            if let Expr::Ident(base_id) = base.as_ref() {
                if let Some(Binding::StdlibModule(m)) = self
                    .checked
                    .typed
                    .resolved
                    .bindings
                    .get(&base_id.span)
                    .cloned()
                {
                    let opcode = stdlib_method_opcode(m, &field.name);
                    let arg_vs: Vec<Value> = args.iter().map(|a| self.lower_expr(fb, a)).collect();
                    return fb.emit_instr(
                        opcode,
                        arg_vs,
                        Some(result_ty),
                        dom,
                        span,
                        InstrMetadata {
                            stdlib_call: Some(StdlibCall::Module {
                                module: m,
                                method: field.name.clone(),
                            }),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        // External contract call: IName.at(addr).method(args).
        // Shape: Call { callee: FieldAccess { base: Call { callee: FieldAccess {
        //   base: Ident(contract), field: "at" }, args: [addr] }, field: method }, args }
        if let Expr::FieldAccess {
            base: at_call,
            field: method_name,
            ..
        } = callee
        {
            if let Expr::Call {
                callee: at_callee,
                args: at_args,
                ..
            } = at_call.as_ref()
            {
                if let Expr::FieldAccess {
                    base: contract_ident,
                    field: at_field,
                    ..
                } = at_callee.as_ref()
                {
                    if at_field.name.as_ref() == "at" {
                        if let Expr::Ident(cname) = contract_ident.as_ref() {
                            let contract_name = cname.name.clone();
                            let func_name = method_name.name.clone();
                            if let Some(funcs) = self.ext_by_name.get(&contract_name) {
                                if let Some((abi_sig, is_view, param_count)) =
                                    funcs.get(&func_name).cloned()
                                {
                                    let addr_v = if let Some(addr_expr) = at_args.first() {
                                        self.lower_expr(fb, addr_expr)
                                    } else {
                                        fb.emit_const(IrConstant::Integer(0), Ty::Address, span)
                                    };
                                    let arg_vs: Vec<Value> =
                                        args.iter().map(|a| self.lower_expr(fb, a)).collect();
                                    let mut operands = vec![addr_v];
                                    operands.extend(arg_vs);
                                    return fb.emit_instr(
                                        Opcode::ExternalCall {
                                            abi_sig,
                                            is_view,
                                            arg_count: param_count,
                                        },
                                        operands,
                                        Some(result_ty),
                                        dom,
                                        span,
                                        InstrMetadata::default(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: emit a void-like no-op. Phase 3 should have emitted the
        // appropriate diagnostic.
        self.diagnostics.push(diag::user_call(span));
        fb.emit_const(IrConstant::Integer(0), result_ty, span)
    }

    // -----------------------------------------------------------------
    // External contract lowering
    // -----------------------------------------------------------------

    fn lower_external_contracts(&mut self, decls: &[covenant_parser::ast::ExternalContractDecl]) {
        for ec in decls {
            let mut func_map: ExtFuncMap = HashMap::new();
            let mut ir_funcs = Vec::new();
            for f in &ec.functions {
                let param_types: Vec<String> = f
                    .params
                    .iter()
                    .map(|p| ast_type_to_abi_str(&p.ty).to_string())
                    .collect();
                let abi_sig: Box<str> =
                    format!("{}({})", f.name.name, param_types.join(",")).into();
                let param_count = f.params.len() as u32;
                func_map.insert(
                    f.name.name.clone(),
                    (abi_sig.clone(), f.is_view, param_count),
                );
                ir_funcs.push(IrExternalFunc {
                    name: f.name.clone(),
                    abi_sig,
                    is_view: f.is_view,
                    param_count,
                });
            }
            self.ext_by_name.insert(ec.name.name.clone(), func_map);
            self.external_contracts.push(IrExternalContract {
                name: ec.name.clone(),
                functions: ir_funcs,
            });
        }
    }
}

// ---------------------------------------------------------------------
// Per-function builder helper
// ---------------------------------------------------------------------

struct FunctionBuilder {
    id: FunctionId,
    name: Ident,
    kind: IrFunctionKind,
    params: Vec<IrParam>,
    returns: Option<Ty>,
    guards: Vec<IrGuard>,
    qualifiers: Vec<IrActionQualifier>,
    annotations: Vec<IrAnnotation>,
    blocks: Vec<IrBlock>,
    entry: BlockId,
    current: BlockId,
    next_value: u32,
    values: Vec<(Value, ValueInfo)>,
    value_types: HashMap<Value, Ty>,
    value_privacy: HashMap<Value, PrivacyDomain>,
    local_to_value: HashMap<LocalId, Value>,
    value_spans: HashMap<Value, Span>,
    span: Span,
}

impl FunctionBuilder {
    fn new(id: FunctionId, name: Ident, kind: IrFunctionKind, span: Span) -> Self {
        let entry = BlockId(0);
        let entry_block = IrBlock {
            id: entry,
            params: vec![],
            instructions: vec![],
            terminator: Terminator::Return(None),
            span,
        };
        Self {
            id,
            name,
            kind,
            params: Vec::new(),
            returns: None,
            guards: Vec::new(),
            qualifiers: Vec::new(),
            annotations: Vec::new(),
            blocks: vec![entry_block],
            entry,
            current: entry,
            next_value: 0,
            values: Vec::new(),
            value_types: HashMap::new(),
            value_privacy: HashMap::new(),
            local_to_value: HashMap::new(),
            value_spans: HashMap::new(),
            span,
        }
    }

    fn new_block(&mut self, span: Span) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(IrBlock {
            id,
            params: vec![],
            instructions: vec![],
            terminator: Terminator::Unreachable,
            span,
        });
        id
    }

    fn set_current(&mut self, block: BlockId) {
        self.current = block;
    }

    fn current_block(&self) -> &IrBlock {
        &self.blocks[self.current.0 as usize]
    }

    fn current_block_mut(&mut self) -> &mut IrBlock {
        &mut self.blocks[self.current.0 as usize]
    }

    fn block_mut(&mut self, id: BlockId) -> &mut IrBlock {
        &mut self.blocks[id.0 as usize]
    }

    fn fresh_value(&mut self, info: ValueInfo) -> Value {
        let v = Value(self.next_value);
        self.next_value += 1;
        self.values.push((v, info));
        v
    }

    fn emit_const(&mut self, c: IrConstant, ty: Ty, span: Span) -> Value {
        let v = self.fresh_value(ValueInfo::Const(c));
        self.value_types.insert(v, ty.clone());
        self.value_privacy
            .insert(v, covenant_privacy::domain_of(&ty));
        self.value_spans.insert(v, span);
        v
    }

    fn emit_instr(
        &mut self,
        opcode: Opcode,
        operands: Vec<Value>,
        result_ty: Option<Ty>,
        result_privacy: PrivacyDomain,
        span: Span,
        metadata: InstrMetadata,
    ) -> Value {
        let has_result = result_ty.is_some();
        let result_v = if has_result {
            let instr_idx = self.current_block().instructions.len() as u32;
            let current = self.current;
            let v = self.fresh_value(ValueInfo::InstrResult {
                instr_idx,
                block: current,
            });
            if let Some(ty) = &result_ty {
                self.value_types.insert(v, ty.clone());
            }
            self.value_privacy.insert(v, result_privacy);
            self.value_spans.insert(v, span);
            Some(v)
        } else {
            None
        };
        let instr = Instr {
            result: result_v,
            opcode,
            operands,
            metadata,
            span,
        };
        self.current_block_mut().instructions.push(instr);
        result_v.unwrap_or(Value(u32::MAX))
    }

    fn close_with_default_return(&mut self, v: Option<Value>) {
        if matches!(self.current_block().terminator, Terminator::Unreachable) {
            self.current_block_mut().terminator = Terminator::Return(v);
        }
    }

    fn finish(self) -> IrFunction {
        IrFunction {
            id: self.id,
            name: self.name,
            kind: self.kind,
            params: self.params,
            returns: self.returns,
            guards: self.guards,
            qualifiers: self.qualifiers,
            annotations: self.annotations,
            blocks: self.blocks,
            entry: self.entry,
            values: self.values,
            value_types: self.value_types,
            value_privacy: self.value_privacy,
            local_to_value: self.local_to_value,
            value_spans: self.value_spans,
            span: self.span,
        }
    }
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn choose_binop(op: BinaryOp, lhs: &Ty, rhs: &Ty) -> Opcode {
    use BinaryOp::*;

    let lhs_is_ct = matches!(lhs, Ty::Ciphertext(_));
    let rhs_is_ct = matches!(rhs, Ty::Ciphertext(_));
    let any_ct = lhs_is_ct || rhs_is_ct;

    let is_time = matches!(lhs, Ty::Time) || matches!(rhs, Ty::Time);
    let is_duration = matches!(lhs, Ty::Duration) || matches!(rhs, Ty::Duration);

    match op {
        Add if any_ct => Opcode::FheAdd,
        Sub if any_ct => Opcode::FheSub,
        Mul if any_ct => Opcode::FheMul,
        Add if is_time => Opcode::TimeAdd,
        Sub if is_time => Opcode::TimeSub,
        Add if is_duration => Opcode::DurationAdd,
        Sub if is_duration => Opcode::DurationSub,
        Mul if is_duration => Opcode::DurationScale,
        Add => Opcode::AddChecked,
        Sub => Opcode::SubChecked,
        Mul => Opcode::MulChecked,
        Div => Opcode::Div,
        Mod => Opcode::Mod,
        Eq if any_ct => Opcode::FheCmpEq,
        NotEq if any_ct => Opcode::FheCmpNe,
        Lt if any_ct => Opcode::FheCmpLt,
        LtEq if any_ct => Opcode::FheCmpLe,
        Gt if any_ct => Opcode::FheCmpGt,
        GtEq if any_ct => Opcode::FheCmpGe,
        Eq => Opcode::Eq,
        NotEq => Opcode::Ne,
        Lt => Opcode::Lt,
        LtEq => Opcode::Le,
        Gt => Opcode::Gt,
        GtEq => Opcode::Ge,
        And if any_ct => Opcode::FheAnd,
        Or if any_ct => Opcode::FheOr,
        And => Opcode::LogicalAnd,
        Or => Opcode::LogicalOr,
        BitAnd => Opcode::BitAnd,
        BitOr => Opcode::BitOr,
        BitXor => Opcode::BitXor,
        Shl => Opcode::ShiftLeft,
        Shr => Opcode::ShiftRight,
        Concat => match lhs {
            Ty::Text => Opcode::TextConcat,
            Ty::Bytes => Opcode::BytesConcat,
            Ty::List(_) => Opcode::ListConcat,
            _ => Opcode::TextConcat,
        },
        // `in` is intercepted and fail-loud-refused (E426) at the sole call
        // site above, before `choose_binop` is ever reached, because membership
        // is a `ListContains` loop that a single-opcode mapping cannot express.
        // Reaching here means that guard was bypassed — a compiler bug, never a
        // silent scalar `Eq` again.
        In => unreachable!("`in` must be refused with E426 before choose_binop"),
    }
}

fn lower_ast_type_impl(t: &covenant_parser::ast::Type) -> Ty {
    use covenant_parser::ast::Type as AT;
    match t {
        AT::Amount(_) => Ty::Amount,
        AT::Time(_) => Ty::Time,
        AT::Duration(_) => Ty::Duration,
        AT::Hash(_) => Ty::Hash,
        AT::Text(_) => Ty::Text,
        AT::Address(_) => Ty::Address,
        AT::PqKey(_) => Ty::PqKey,
        AT::Bytes(_) => Ty::Bytes,
        AT::Bool(_) => Ty::Bool,
        AT::Choice(_, _) => Ty::Unknown,
        AT::Encrypted(inner, _) => Ty::Ciphertext(Box::new(lower_ast_type_impl(inner))),
        AT::List(inner, _) => Ty::List(Box::new(lower_ast_type_impl(inner))),
        AT::Map(k, v, _) => Ty::Map(
            Box::new(lower_ast_type_impl(k)),
            Box::new(lower_ast_type_impl(v)),
        ),
        AT::PriorityQueue(_, _, _, _) => Ty::Unknown,
        AT::Shares { .. } => Ty::Unknown,
        AT::User(_) => Ty::Unknown,
    }
}

fn stdlib_fn_opcode(f: StdlibFn) -> Opcode {
    match f {
        StdlibFn::Keccak => Opcode::Keccak,
        StdlibFn::Encode => Opcode::AbiEncode,
        StdlibFn::Decode => Opcode::AbiDecode,
        StdlibFn::Encrypted => Opcode::FheEncryptTrivial,
        StdlibFn::CiphertextHashOf => Opcode::FheCiphertextHash,
        StdlibFn::Destroy => Opcode::DestructionProof,
        StdlibFn::Freeze => Opcode::DestructionProof,
        StdlibFn::RandPq => Opcode::PqRand,
        // Min/Max/Abs/Pow/Sqrt are intentionally absent: they are rejected in
        // `lower_call` via `unimplemented_math_name` before reaching here.
        // Do NOT add placeholder arms — that is exactly how `max(a, b)` came
        // to compile as `a + b`.
        _ => Opcode::AbiEncode,
    }
}

/// The math builtins that have no lowering. Returning `Some(name)` makes the
/// call a hard compile error instead of silently emitting the wrong opcode.
///
/// `min`/`max`/`abs` need a compare-and-branch (the IR has `Lt`/`Gt` but no
/// select, and `stdlib_fn_opcode` can only return a single opcode), and
/// `pow`/`sqrt` need loops. Until that multi-block lowering exists, refusing
/// is the only behaviour that cannot produce a wrong contract.
fn unimplemented_math_name(f: StdlibFn) -> Option<&'static str> {
    match f {
        StdlibFn::Min => Some("min"),
        StdlibFn::Max => Some("max"),
        StdlibFn::Abs => Some("abs"),
        StdlibFn::Pow => Some("pow"),
        StdlibFn::Sqrt => Some("sqrt"),
        _ => None,
    }
}

fn stdlib_method_opcode(m: StdlibModule, method: &str) -> Opcode {
    match (m, method) {
        (StdlibModule::PQKeys, "verify_dilithium") => Opcode::PqVerifyDilithium,
        (StdlibModule::PQKeys, "hybrid_verify") => Opcode::PqHybridVerify,
        (StdlibModule::PQKeys, "rand_pq") => Opcode::PqRand,
        (StdlibModule::PQKeys, "encapsulate") => Opcode::KyberEncrypt,
        (StdlibModule::EncryptedTokens, "add_encrypted") => Opcode::FheAdd,
        (StdlibModule::EncryptedTokens, "compare_encrypted") => Opcode::FheCmpLt,
        (StdlibModule::EncryptedTokens, "encrypt_with") => Opcode::FheEncryptFresh,
        (StdlibModule::FHEVerification, "verify") => Opcode::ZkVerify,
        (StdlibModule::FHEVerification, "verify_vdf") => Opcode::VdfVerify,
        (StdlibModule::FHEVerification, "prove") => Opcode::ZkProofPayload,
        (StdlibModule::Amnesia, "begin_destruction") => Opcode::AmnesiaBegin,
        (StdlibModule::Amnesia, "submit_share") => Opcode::AmnesiaSubmitShare,
        (StdlibModule::Amnesia, "finalize_destruction") => Opcode::AmnesiaFinalize,
        (StdlibModule::Amnesia, "reconstruct") => Opcode::ShamirReconstruct,
        (StdlibModule::Crypto, "keccak") => Opcode::Keccak,
        (StdlibModule::Crypto, "sha256") | (StdlibModule::Crypto, "blake2b") => Opcode::Blake2,
        (StdlibModule::Crypto, "hmac") => Opcode::Hmac,
        (StdlibModule::Encoding, "encode") => Opcode::AbiEncode,
        (StdlibModule::Encoding, "decode") => Opcode::AbiDecode,
        (StdlibModule::Encoding, "abi_pack") => Opcode::AbiPack,
        _ => Opcode::AbiEncode,
    }
}

fn lower_anchor(a: &AnchorClause) -> AnchorInfo {
    AnchorInfo {
        chains: a.chains.iter().map(|t| t.value.clone()).collect(),
        span: a.span,
    }
}

fn lower_upgradeable(u: &UpgradeableClause) -> UpgradeableInfo {
    UpgradeableInfo {
        principal: u.principal.clone(),
        span: u.span,
    }
}

/// Extract the integer value from a `@slot(N)` annotation attached to a field
/// declaration. Returns `None` if no such annotation is present. Emits an
/// error diagnostic on malformed `@slot(...)` usage.
///
/// KSR-CVN-021 remediation: the codegen side must honor this value instead of
/// assigning a sequential slot.
fn extract_slot_annotation(
    annotations: &[covenant_parser::ast::Annotation],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<u32> {
    let mut found: Option<u32> = None;
    for ann in annotations {
        if ann.name.name.as_ref() != "slot" {
            continue;
        }
        if ann.args.len() != 1 {
            diagnostics.push(diag::slot_annotation_invalid(
                ann.span,
                "expected exactly one positional integer argument",
            ));
            continue;
        }
        match &ann.args[0] {
            AnnotationArg::Positional(Expr::Literal(LiteralExpr::Integer(n, _))) => {
                if *n > u32::MAX as u128 {
                    diagnostics.push(diag::slot_annotation_invalid(
                        ann.span,
                        "slot index does not fit in u32",
                    ));
                    continue;
                }
                if found.is_some() {
                    diagnostics.push(diag::slot_annotation_invalid(
                        ann.span,
                        "duplicate `@slot(...)` on the same field",
                    ));
                    continue;
                }
                found = Some(*n as u32);
            }
            _ => {
                diagnostics.push(diag::slot_annotation_invalid(
                    ann.span,
                    "argument must be an integer literal",
                ));
            }
        }
    }
    found
}

/// KSR-CVN-030: the canonical set of accepted annotation names. Any name
/// outside this set triggers a `W850` warning with a Levenshtein suggestion.
///
/// Keep this in sync with:
/// * `lower_annotation` below (builder-consumed names),
/// * `extract_slot_annotation` (handles `@slot(N)`),
/// * `covenant-evm-backend/src/codegen.rs` (`non_reentrant`, `initializer`).
const KNOWN_ANNOTATIONS: &[&str] = &[
    "precompute",
    "batch_up_to",
    "prove_offchain",
    "gas_budget",
    "slot",
    "non_reentrant",
    "initializer",
    "test",
    "vdf_locked",
];

fn levenshtein(a: &str, b: &str) -> usize {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let (n, m) = (ac.len(), bc.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = usize::from(ac[i - 1] != bc[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

fn suggest_annotation(name: &str) -> Option<&'static str> {
    let mut best: Option<(&'static str, usize)> = None;
    for cand in KNOWN_ANNOTATIONS {
        if cand.len() < 3 {
            continue;
        }
        let d = levenshtein(name, cand);
        if d <= 2 && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((cand, d));
        }
    }
    best.map(|(s, _)| s)
}

fn validate_annotation_name(
    ann: &covenant_parser::ast::Annotation,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let name = ann.name.name.as_ref();
    if KNOWN_ANNOTATIONS.contains(&name) {
        return;
    }
    // Case-insensitive match against the canonical set — if the user wrote
    // `@NonReentrant`, that is a near-miss, not a custom annotation.
    let lower = name.to_ascii_lowercase();
    let sugg = if KNOWN_ANNOTATIONS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(&lower))
    {
        KNOWN_ANNOTATIONS
            .iter()
            .find(|k| k.eq_ignore_ascii_case(&lower))
            .copied()
    } else {
        suggest_annotation(name)
    };
    diagnostics.push(diag::unknown_annotation(ann.name.span, name, sugg));
}

/// Convert a constant field-default expression into an `IrConstant` the
/// constructor can SSTORE, or `None` when it is not a supported constant.
///
/// Only the literal types the EVM backend's `emit_const_initializer` stores
/// correctly in a single word are carried: integers, bools, and 20/32-byte
/// hex (address / hash). `Text`, `Duration` and non-20/32-byte hex hit that
/// function's `vec![0]` fallback, so populating them would store a WRONG
/// value — worse than dropping the default. They stay `None` and reach the
/// runtime as 0, exactly as before; honouring them needs proper (dynamic or
/// duration-folding) constructor encoding, tracked in `DEBT.md`. Computed
/// defaults (`= a + b`) are not constants and also return `None`.
fn field_default_const(expr: &Expr) -> Option<IrConstant> {
    let Expr::Literal(lit) = expr else {
        return None;
    };
    match lit {
        LiteralExpr::Integer(n, _) => Some(IrConstant::Integer(*n)),
        LiteralExpr::Bool(b, _) => Some(IrConstant::Bool(*b)),
        LiteralExpr::Hex(bytes, _) => match bytes.len() {
            20 => {
                let mut arr = [0u8; 20];
                arr.copy_from_slice(bytes);
                Some(IrConstant::Address(Box::new(arr)))
            }
            32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(bytes);
                Some(IrConstant::Hash(Box::new(arr)))
            }
            _ => None,
        },
        // Backend can't store these in one word yet — leave as-is rather than
        // store a wrong value.
        LiteralExpr::Text(_, _) | LiteralExpr::Duration(_, _, _) => None,
    }
}

/// Is this action a test? `test_`-prefixed name, or an explicit `@test`.
///
/// SINGLE SOURCE OF TRUTH. `covenant test`'s runner must call this rather than
/// re-implement the predicate: if the runner and the backend disagree about
/// what a test is, either a test becomes unrunnable or a test entrypoint
/// ships. Note the runner additionally requires zero args to *run* a test, but
/// classification here is deliberately name/annotation only — a parameterised
/// `test_*` action is not runnable, so leaving it a public `Action` would be
/// the worst case (an un-runnable public mutator).
pub fn is_test_action(a: &covenant_parser::ast::ActionDecl) -> bool {
    a.name.name.as_ref().starts_with("test_")
        || a.annotations
            .iter()
            .any(|ann| ann.name.name.as_ref() == "test")
}

fn lower_annotation(ann: &covenant_parser::ast::Annotation) -> IrAnnotation {
    match ann.name.name.as_ref() {
        "precompute" => {
            // Lose the expression here — it's metadata; the builder can revisit.
            IrAnnotation::Unknown("precompute".into())
        }
        "batch_up_to" => {
            let n = ann
                .args
                .iter()
                .find_map(|a| match a {
                    AnnotationArg::Positional(Expr::Literal(LiteralExpr::Integer(n, _))) => {
                        Some(*n as u32)
                    }
                    _ => None,
                })
                .unwrap_or(0);
            IrAnnotation::BatchUpTo(n)
        }
        "prove_offchain" => IrAnnotation::ProveOffchain,
        "gas_budget" => IrAnnotation::GasBudget {
            l1: None,
            pgas: None,
        },
        other => IrAnnotation::Unknown(other.into()),
    }
}

/// Silence a `ConstructKind` warning; used in construct-kind dispatch stubs.
#[allow(dead_code)]
fn _construct_link(_c: ConstructKind) {}

/// Map an AST `Type` to its canonical Solidity ABI type string for use in
/// external-function signature construction.
fn ast_type_to_abi_str(ty: &covenant_parser::ast::Type) -> &'static str {
    use covenant_parser::ast::Type;
    match ty {
        Type::Amount(_) | Type::Time(_) | Type::Duration(_) => "uint256",
        Type::Address(_) => "address",
        Type::Bool(_) => "bool",
        Type::Hash(_) => "bytes32",
        Type::Bytes(_) => "bytes",
        Type::Text(_) => "string",
        Type::PqKey(_) => "bytes",
        _ => "bytes",
    }
}
